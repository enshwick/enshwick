//! Crash attribution: when a hardware fault happens in a mod's code, or in the
//! game with a mod's code on the stack, `logs/enshwick/crash.log` names the
//! mod before the game's own handler runs. The fault itself goes on as it
//! would without the loader.

use std::io::Write as _;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU32, Ordering};

use windows_sys::Win32::Foundation::{
    EXCEPTION_ACCESS_VIOLATION, EXCEPTION_ILLEGAL_INSTRUCTION, EXCEPTION_INT_DIVIDE_BY_ZERO,
    EXCEPTION_PRIV_INSTRUCTION, EXCEPTION_STACK_OVERFLOW, HMODULE, NTSTATUS,
};
use windows_sys::Win32::System::Diagnostics::Debug::{
    AddVectoredExceptionHandler, EXCEPTION_POINTERS,
};
use windows_sys::Win32::System::ProcessStatus::{GetModuleInformation, MODULEINFO};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetCurrentThreadStackLimits};

/// Keeps looking for another handler: the loader only records.
const CONTINUE_SEARCH: i32 = 0;
/// Reports per session; a game that handles its own faults stays quiet.
const MAX_REPORTS: u32 = 5;
/// Stack words searched for a mod's return address.
const STACK_WORDS: usize = 4096;

/// A module's address range and whom it belongs to.
#[derive(Debug)]
struct Range {
    start: usize,
    end: usize,
    owner: String,
}

struct Crash {
    ranges: Vec<Range>,
    file: PathBuf,
}

static CRASH: OnceLock<Crash> = OnceLock::new();
static REPORTS: AtomicU32 = AtomicU32::new(0);

/// The address range of a loaded module, `None` when Windows does not say.
pub(crate) fn range_of(module: HMODULE) -> Option<(usize, usize)> {
    let mut info = MODULEINFO {
        lpBaseOfDll: core::ptr::null_mut(),
        SizeOfImage: 0,
        EntryPoint: core::ptr::null_mut(),
    };
    // SAFETY: no preconditions; the pseudo handle needs no closing.
    let process = unsafe { GetCurrentProcess() };
    // SAFETY: a loaded module and a writable struct of the given size.
    let ok = unsafe {
        GetModuleInformation(
            process,
            module,
            &raw mut info,
            u32::try_from(size_of::<MODULEINFO>()).unwrap_or(0),
        )
    };
    let start = info.lpBaseOfDll.addr();
    (ok != 0).then(|| (start, start + info.SizeOfImage as usize))
}

/// Starts recording faults for `modules`, `(owner, start, end)`, into `file`.
pub(crate) fn install(modules: Vec<(String, usize, usize)>, file: PathBuf) {
    let ranges = modules
        .into_iter()
        .map(|(owner, start, end)| Range { start, end, owner })
        .collect();
    if CRASH.set(Crash { ranges, file }).is_err() {
        return;
    }
    // SAFETY: the handler is a plain function that never unwinds.
    let handle = unsafe { AddVectoredExceptionHandler(1, Some(handler)) };
    if handle.is_null() {
        crate::log!(WARN, "crash attribution off: no exception handler");
    }
}

fn owner(ranges: &[Range], address: usize) -> Option<&Range> {
    ranges.iter().find(|r| (r.start..r.end).contains(&address))
}

/// A mod or loader range holding a return address in the top of the stack.
fn on_stack(ranges: &[Range], rsp: usize) -> Option<&Range> {
    let (mut low, mut high) = (0usize, 0usize);
    // SAFETY: two writable words.
    unsafe { GetCurrentThreadStackLimits(&raw mut low, &raw mut high) };
    if !(low..high).contains(&rsp) {
        return None;
    }
    let words = ((high - rsp) / 8).min(STACK_WORDS);
    (0..words).find_map(|i| {
        // SAFETY: between the stack pointer and the stack's base, committed.
        let word = unsafe { core::ptr::with_exposed_provenance::<usize>(rsp + i * 8).read() };
        owner(ranges, word)
    })
}

unsafe extern "system" fn handler(pointers: *mut EXCEPTION_POINTERS) -> i32 {
    let Some(crash) = CRASH.get() else {
        return CONTINUE_SEARCH;
    };
    // SAFETY: Windows passes valid exception pointers to a vectored handler.
    let Some(pointers) = (unsafe { pointers.as_ref() }) else {
        return CONTINUE_SEARCH;
    };
    // SAFETY: as above.
    let (Some(record), Some(context)) = (unsafe { pointers.ExceptionRecord.as_ref() }, unsafe {
        pointers.ContextRecord.as_ref()
    }) else {
        return CONTINUE_SEARCH;
    };
    let code: NTSTATUS = record.ExceptionCode;
    if ![
        EXCEPTION_ACCESS_VIOLATION,
        EXCEPTION_ILLEGAL_INSTRUCTION,
        EXCEPTION_INT_DIVIDE_BY_ZERO,
        EXCEPTION_PRIV_INSTRUCTION,
        EXCEPTION_STACK_OVERFLOW,
    ]
    .contains(&code)
    {
        return CONTINUE_SEARCH;
    }
    let at = record.ExceptionAddress.addr();
    let (suspect, place) = match owner(&crash.ranges, at) {
        Some(range) => (range, "in its code"),
        // A stack overflow leaves no room to walk the stack.
        None if code != EXCEPTION_STACK_OVERFLOW => {
            let Some(range) = on_stack(&crash.ranges, usize::try_from(context.Rsp).unwrap_or(0))
            else {
                return CONTINUE_SEARCH;
            };
            (range, "in game code called with its code on the stack")
        }
        None => return CONTINUE_SEARCH,
    };
    if REPORTS.fetch_add(1, Ordering::Relaxed) >= MAX_REPORTS {
        return CONTINUE_SEARCH;
    }
    let line = format!(
        "fault {code:#010x} at {at:#x} (+{:#x} in {}), {place}; access {:#x}\n",
        at.wrapping_sub(suspect.start),
        suspect.owner,
        record.ExceptionInformation[1],
    );
    // Straight to disk: the process may end before the log thread writes.
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&crash.file)
    {
        let _written = file.write_all(line.as_bytes());
    }
    crate::log::write(
        enshwick_api::abi::Level::ERROR,
        crate::settings::LOADER_ID,
        line.trim_end(),
    );
    CONTINUE_SEARCH
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_address_belongs_to_the_range_holding_it() {
        let ranges = [
            Range {
                start: 0x1000,
                end: 0x2000,
                owner: "a".into(),
            },
            Range {
                start: 0x5000,
                end: 0x6000,
                owner: "b".into(),
            },
        ];
        assert_eq!(owner(&ranges, 0x5800).map(|r| r.owner.as_str()), Some("b"));
        assert!(owner(&ranges, 0x2000).is_none());
    }
}
