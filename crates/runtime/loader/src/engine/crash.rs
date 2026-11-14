//! Crash blame: when the process dies of a fault nothing handled, the loader
//! names the mod whose code faulted, whose callback the loader was running,
//! or whose code was on the faulting thread's stack, in the loader's log
//! file and in `logs/enshwick-crash_<date>_<time>.log`, and the next start
//! shows it in its log and the status summary.
//!
//! Only a fault that ends the process is reported: the loader writes when
//! the process's last-chance filter runs, or when the game's crash handler
//! writes its crash dump, never for a fault the game or a mod handles. A
//! first-chance handler only notes, for each fault, which mod callback the
//! faulting thread was running, so the report can name it from another
//! thread. The report runs in a dying process, so it allocates nothing,
//! formats into a buffer of its own and writes through file handles opened
//! at start, then hands the fault on: to the filter the loader replaced, or
//! to the game's dump writer. The status thread puts the loader's filter
//! back on top whenever another filter took its place.

use std::cell::UnsafeCell;
use std::path::Path;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};

use enshwick_core::abi::HookFlags;
use unwind::{rewind, unwind, write};
use windows_sys::Win32::Foundation::{
    EXCEPTION_ACCESS_VIOLATION, EXCEPTION_ILLEGAL_INSTRUCTION, EXCEPTION_INT_DIVIDE_BY_ZERO,
    EXCEPTION_PRIV_INSTRUCTION, EXCEPTION_STACK_OVERFLOW, GENERIC_READ, GENERIC_WRITE, HANDLE,
    HMODULE, INVALID_HANDLE_VALUE, NTSTATUS, SYSTEMTIME,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_APPEND_DATA, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE,
    OPEN_ALWAYS,
};
use windows_sys::Win32::System::Diagnostics::Debug::{
    AddVectoredExceptionHandler, EXCEPTION_POINTERS, LPTOP_LEVEL_EXCEPTION_FILTER,
    SetUnhandledExceptionFilter,
};
use windows_sys::Win32::System::Memory::MEM_IMAGE;
use windows_sys::Win32::System::ProcessStatus::{
    GetMappedFileNameW, GetModuleInformation, MODULEINFO,
};
use windows_sys::Win32::System::SystemInformation::GetSystemTime;
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentProcessId, GetCurrentThreadId,
};

use crate::engine::loader::{Loader, Target};
use crate::engine::mod_index::ModIndex;

/// Keeps looking for another handler: the default ends the process.
const CONTINUE_SEARCH: i32 = 0;
/// Frames the filter unwinds looking for a mod's code.
const FRAMES: usize = 48;
/// The longest name kept for a range.
const NAME: usize = 64;
/// The crash line's room.
const LINE: usize = 768;
/// The source the crash files are named after in `logs/`.
const CRASH_SOURCE: &str = "enshwick-crash";
/// Crash files kept; older ones are deleted at start.
const KEPT_CRASHES: usize = 10;
/// The longest crash file path, in UTF-16 units with its NUL.
const CRASH_PATH: usize = 1024;
/// The longest mapped file path the crash line reads a module's name from, in
/// UTF-16 units, kept small for the stack of a dying thread.
const MAPPED_PATH: usize = 512;

/// A module's address range and whom it belongs to, kept without
/// allocation for the filter.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Range {
    /// The first byte.
    pub start: usize,
    /// One past the last byte.
    pub end: usize,
    /// The mod's index, [`ModIndex::LOADER`] for the loader.
    pub owner: ModIndex,
    name: [u8; NAME],
    len: usize,
}

impl Range {
    /// A range named `name`, cut at 64 bytes.
    #[must_use]
    pub(crate) fn new(start: usize, end: usize, owner: ModIndex, name: &str) -> Self {
        let mut bytes = [0; NAME];
        let len = name.len().min(NAME);
        bytes[..len].copy_from_slice(&name.as_bytes()[..len]);
        Self {
            start,
            end,
            owner,
            name: bytes,
            len,
        }
    }

    /// The owner's name.
    #[must_use]
    pub(crate) fn name(&self) -> &[u8] {
        &self.name[..self.len]
    }

    fn holds(&self, address: usize) -> bool {
        (self.start..self.end).contains(&address)
    }
}

/// Whom a fault is blamed on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Blame {
    /// It faulted in a mod's code: the range.
    Code(usize),
    /// The loader was running a mod's callback: the mod's range, if known,
    /// the mod's index and the feature.
    Callback(Option<usize>, usize, &'static str),
    /// A mod's code was on the stack: the range and the frame.
    Stack(usize, usize),
    /// It faulted in the game under the loader's code, with no mod's on the
    /// stack: the loader's range, the frame and its return address. Not
    /// blamed on the loader, since its pass-through detours sit under most
    /// game code.
    Under(usize, usize, usize),
    /// It faulted in the loader, with no mod's callback running or code on
    /// the stack.
    Loader(usize),
    /// Neither a mod's code nor the loader's anywhere near it, over a stack
    /// searched whole, in a module's code.
    Game,
    /// No mod's or loader's code found, but the search could not rule them
    /// out: the words that say why.
    Unknown(&'static str),
}

/// What a crash line says when the walk found no mod's or loader's code but
/// did not search the whole stack: an overflowed one, or one deeper than
/// [`FRAMES`].
pub(crate) const NOT_WALKED: &str =
    "whose code led there is not known: the stack was not searched whole";

/// What a crash line says when the fault lies in code no module names, such
/// as a trampoline, a hook stub or code a mod generated.
pub(crate) const NO_MODULE: &str =
    "in code no module names, with no mod's or loader's code on the stack";

/// The blame [`blame`] gave, kept as the game's only when `walked` says the
/// whole stack was searched and `named` that a module holds the fault.
#[must_use]
pub(crate) const fn settle(blame: Blame, walked: bool, named: bool) -> Blame {
    match blame {
        Blame::Game if !walked => Blame::Unknown(NOT_WALKED),
        Blame::Game if !named => Blame::Unknown(NO_MODULE),
        other => other,
    }
}

/// Whether the crash line looks up the module `at` lies in: only when no
/// range holds it, and never on an overflowed stack, which has no room for
/// the lookup.
#[must_use]
pub(crate) fn names_module(ranges: &[Range], at: usize, code: NTSTATUS) -> bool {
    code != EXCEPTION_STACK_OVERFLOW && !ranges.iter().any(|r| r.holds(at))
}

/// Whom the fault at `at` is blamed on, from the ranges, the callback the
/// loader was running, and the return addresses of the stack, innermost
/// first.
#[must_use]
pub(crate) fn blame(
    ranges: &[Range],
    at: usize,
    callback: Option<(usize, &'static str)>,
    frames: &[usize],
) -> Blame {
    let range_of = |address: usize| ranges.iter().position(|r| r.holds(address));
    let at_range = range_of(at);
    if let Some(index) = at_range
        && ranges[index].owner != ModIndex::LOADER
    {
        return Blame::Code(index);
    }
    if let Some((owner, feature)) = callback {
        return Blame::Callback(
            ranges.iter().position(|r| r.owner == ModIndex::new(owner)),
            owner,
            feature,
        );
    }
    let on_stack = frames.iter().enumerate().find_map(|(frame, &address)| {
        range_of(address)
            .filter(|&i| ranges[i].owner != ModIndex::LOADER)
            .map(|i| (i, frame))
    });
    if let Some((index, frame)) = on_stack {
        return Blame::Stack(index, frame);
    }
    if let Some(index) = at_range {
        return Blame::Loader(index);
    }
    frames
        .iter()
        .enumerate()
        .find_map(|(frame, &address)| range_of(address).map(|i| Blame::Under(i, frame, address)))
        .unwrap_or(Blame::Game)
}

/// A line built without allocation.
#[derive(Debug)]
pub(crate) struct Line {
    bytes: [u8; LINE],
    len: usize,
}

impl Default for Line {
    fn default() -> Self {
        Self {
            bytes: [0; LINE],
            len: 0,
        }
    }
}

impl Line {
    /// Adds `text`, as much as fits.
    pub(crate) fn text(&mut self, text: &[u8]) -> &mut Self {
        let room = LINE - self.len;
        let n = text.len().min(room);
        self.bytes[self.len..self.len + n].copy_from_slice(&text[..n]);
        self.len += n;
        self
    }

    /// Adds `value` as `0x…` in lower-case hex.
    pub(crate) fn hex(&mut self, value: u64) -> &mut Self {
        let mut digits = [0u8; 18];
        digits[0] = b'0';
        digits[1] = b'x';
        let mut n = 2;
        let mut started = false;
        for shift in (0..16).rev() {
            let nibble = ((value >> (shift * 4)) & 0xf) as u8;
            if nibble != 0 || started || shift == 0 {
                started = true;
                digits[n] = b"0123456789abcdef"[nibble as usize];
                n += 1;
            }
        }
        self.text(&digits[..n])
    }

    /// Adds `value` in decimal, padded with zeros to `width` digits.
    pub(crate) fn dec(&mut self, value: u64, width: usize) -> &mut Self {
        let mut digits = [0u8; 20];
        let mut n = 0;
        let mut rest = value;
        loop {
            digits[n] = b'0' + (rest % 10) as u8;
            n += 1;
            rest /= 10;
            if rest == 0 && n >= width {
                break;
            }
        }
        digits[..n].reverse();
        self.text(&digits[..n])
    }

    /// The line so far.
    #[must_use]
    pub(crate) fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
}

/// The fault in words, as the crash file keeps it: time, code, address and
/// the module it lies in when no range holds it, whom it is blamed on and
/// where, and what was accessed.
#[expect(
    clippy::too_many_arguments,
    reason = "the parts of one crash line, each read where the fault is"
)]
pub(crate) fn describe(
    line: &mut Line,
    time: [u16; 6],
    code: u32,
    at: usize,
    module: Option<(usize, &[u8])>,
    access: Option<(u64, u64)>,
    blame: Blame,
    ranges: &[Range],
) {
    let [year, month, day, hour, minute, second] = time.map(u64::from);
    line.dec(year, 4)
        .text(b"-")
        .dec(month, 2)
        .text(b"-")
        .dec(day, 2);
    line.text(b" ")
        .dec(hour, 2)
        .text(b":")
        .dec(minute, 2)
        .text(b":")
        .dec(second, 2);
    line.text(b"Z fault ")
        .hex(u64::from(code))
        .text(b" at ")
        .hex(at as u64);
    if let Some((base, name)) = module {
        line.text(b" (+")
            .hex(at.wrapping_sub(base) as u64)
            .text(b" in ")
            .text(name)
            .text(b")");
    }
    let offset = |line: &mut Line, range: &Range, address: usize| {
        line.text(b" (+")
            .hex(address.wrapping_sub(range.start) as u64)
            .text(b" in ")
            .text(range.name())
            .text(b")");
    };
    match blame {
        Blame::Code(index) => {
            let range = &ranges[index];
            line.text(b", blamed on ")
                .text(range.name())
                .text(b": in its code");
            offset(line, range, at);
        }
        Blame::Callback(range, owner, feature) => {
            line.text(b", blamed on ");
            match range.map(|i| &ranges[i]) {
                Some(range) => line.text(range.name()),
                None => line.text(b"mod #").dec(owner as u64, 1),
            };
            line.text(b": in its ")
                .text(feature.as_bytes())
                .text(b" callback");
            if let Some(range) = ranges.iter().find(|r| r.holds(at)) {
                offset(line, range, at);
            }
        }
        Blame::Stack(index, frame) => {
            let range = &ranges[index];
            line.text(b", blamed on ")
                .text(range.name())
                .text(b": its code is on the stack, frame ")
                .dec(frame as u64, 1);
        }
        Blame::Under(index, frame, address) => {
            let range = &ranges[index];
            line.text(b", in the game, called from ")
                .text(range.name())
                .text(b": frame ")
                .dec(frame as u64, 1);
            offset(line, range, address);
        }
        Blame::Loader(index) => {
            line.text(b", in the loader");
            offset(line, &ranges[index], at);
        }
        Blame::Game => {
            line.text(b", ")
                .text(crate::engine::diag::GAME_CRASHED.as_bytes());
        }
        Blame::Unknown(words) => {
            line.text(b", ").text(words.as_bytes());
        }
    }
    if let Some((kind, address)) = access {
        let verb: &[u8] = match kind {
            0 => b"reading",
            1 => b"writing",
            8 => b"running",
            _ => b"touching",
        };
        line.text(b"; ").text(verb).text(b" ").hex(address);
    }
    line.text(b"\r\n");
}

/// What the filter needs, set up at start.
struct Setup {
    ranges: Box<[Range]>,
    /// The loader's log file of this run, opened for appending.
    log: usize,
    /// `<logs>\enshwick-crash_` in UTF-16, without a NUL: the crash file's
    /// path up to its time.
    crash_file: Box<[u16]>,
    /// `mods/enshwick/last-crash.txt`, rewritten with the last crash.
    last: usize,
}

/// What crash reports keep for the process: a crash ends it, so no part
/// forgets any of it. Read by the filter and the first-chance handler, so
/// it neither allocates nor locks.
struct State {
    setup: OnceLock<Setup>,
    /// The last crash of the session before, read at start.
    last: OnceLock<String>,
    /// The filter the loader's replaced, called after it.
    next: AtomicUsize,
    /// Set while the filter runs: a fault inside it goes to the next filter.
    inside: AtomicBool,
    /// The thread the filter handed on to the next filter, 0 before: a
    /// filter set after the loader's chains back to it, which must not call
    /// that one again.
    handed_on: AtomicU32,
    /// The filter's line.
    line: Shared,
    /// The latest faults the first-chance handler saw, oldest overwritten.
    noted: [Noted; 8],
    /// The slot of `noted` the next fault takes, modulo its length.
    next_note: AtomicUsize,
}

static STATE: State = State {
    setup: OnceLock::new(),
    last: OnceLock::new(),
    next: AtomicUsize::new(0),
    inside: AtomicBool::new(false),
    handed_on: AtomicU32::new(0),
    line: Shared(UnsafeCell::new(Line {
        bytes: [0; LINE],
        len: 0,
    })),
    noted: [const {
        Noted {
            thread: AtomicU32::new(0),
            address: AtomicUsize::new(0),
            marker: AtomicU32::new(0),
        }
    }; 8],
    next_note: AtomicUsize::new(0),
};

/// The filter's line, used by one thread at a time under the state's
/// `inside` flag.
struct Shared(UnsafeCell<Line>);

// SAFETY: `report` touches the line only after its `inside.swap(true)`
// returned false, which one thread in the process ever sees, so the line is
// never shared.
unsafe impl Sync for Shared {}

/// What the first-chance handler noted of a fault: the thread, the address
/// and the mod callback the thread ran, as [`crate::engine::profile::marker`] gives
/// it.
struct Noted {
    thread: AtomicU32,
    address: AtomicUsize,
    marker: AtomicU32,
}

/// The callback the thread `thread` ran when it faulted at `at`: read here
/// when this is the thread, else from what the first-chance handler noted.
fn noted(thread: u32, at: usize) -> Option<(usize, crate::engine::profile::Feature)> {
    // SAFETY: `GetCurrentThreadId` takes no arguments and has no
    // preconditions.
    if thread == unsafe { GetCurrentThreadId() } {
        return crate::engine::profile::current_callback();
    }
    STATE
        .noted
        .iter()
        .find(|n| {
            n.thread.load(Ordering::Acquire) == thread && n.address.load(Ordering::Acquire) == at
        })
        .and_then(|n| crate::engine::profile::decode(n.marker.load(Ordering::Acquire)))
}

/// Notes the faulting thread's callback, for a report from another thread;
/// handles nothing.
unsafe extern "system" fn first_chance(pointers: *mut EXCEPTION_POINTERS) -> i32 {
    // SAFETY: Windows passes a vectored handler null or the fault's
    // `EXCEPTION_POINTERS`, valid while the handler runs.
    let record = unsafe { pointers.as_ref() }.and_then(|p| {
        // SAFETY: the fault's `EXCEPTION_POINTERS` hold null or its
        // `EXCEPTION_RECORD`, valid while the handler runs.
        unsafe { p.ExceptionRecord.as_ref() }
    });
    if let Some(record) = record
        && FATAL.contains(&record.ExceptionCode)
    {
        let slot =
            &STATE.noted[STATE.next_note.fetch_add(1, Ordering::Relaxed) % STATE.noted.len()];
        slot.marker
            .store(crate::engine::profile::marker(), Ordering::Relaxed);
        slot.address
            .store(record.ExceptionAddress.addr(), Ordering::Relaxed);
        // SAFETY: `GetCurrentThreadId` takes no arguments and has no
        // preconditions.
        slot.thread
            .store(unsafe { GetCurrentThreadId() }, Ordering::Release);
    }
    CONTINUE_SEARCH
}

/// The faults a report is written for when nothing handles them.
const FATAL: [NTSTATUS; 5] = [
    EXCEPTION_ACCESS_VIOLATION,
    EXCEPTION_ILLEGAL_INSTRUCTION,
    EXCEPTION_INT_DIVIDE_BY_ZERO,
    EXCEPTION_PRIV_INSTRUCTION,
    EXCEPTION_STACK_OVERFLOW,
];

/// The exception parameter of `MiniDumpWriteDump`, packed to 4 as in
/// `dbghelp.h`.
#[repr(C, packed(4))]
struct DumpException {
    thread: u32,
    pointers: *const EXCEPTION_POINTERS,
    client_pointers: i32,
}

/// Whether a dump written for a fault coded `code` is of a crash: only the
/// faults [`FATAL`] names are, never the breakpoints, C++ exceptions or
/// reports a handler writes a dump for and survives.
fn fatal(code: NTSTATUS) -> bool {
    FATAL.contains(&code)
}

/// The code of the fault `pointers` names, `None` when either pointer is
/// null.
///
/// # Safety
///
/// - `pointers` is null or points at a fault's `EXCEPTION_POINTERS` in this
///   process, whose `ExceptionRecord` is null or valid.
/// - The faulting thread waits in the fault during the call.
unsafe fn fault_code(pointers: *const EXCEPTION_POINTERS) -> Option<NTSTATUS> {
    // SAFETY: the caller's contract makes `pointers` null or the fault's
    // valid `EXCEPTION_POINTERS`.
    let pointers = unsafe { pointers.as_ref() }?;
    // SAFETY: the caller's contract makes the record null or valid while the
    // faulting thread waits in the fault.
    let record = unsafe { pointers.ExceptionRecord.as_ref() }?;
    Some(record.ExceptionCode)
}

crate::engine::chains::chain_detour! {
    /// The game's crash handler writes a dump through `dbghelp`'s
    /// `MiniDumpWriteDump`: report the fault first when it is one that ends
    /// the process.
    fn dump(
        process: HANDLE,
        process_id: u32,
        file: HANDLE,
        kind: u32,
        exception: *const DumpException,
        streams: *const core::ffi::c_void,
        callback: *const core::ffi::c_void,
    ) -> i32;
    next DUMP_NEXT;
    |onward| {
        // Pointers "in the client" are this process's own when the dump is
        // of this process, as the game's handler writes it.
        // SAFETY: `GetCurrentProcessId` takes no arguments and has no
        // preconditions.
        let ours = process_id == unsafe { GetCurrentProcessId() };
        // SAFETY: the caller's `MINIDUMP_EXCEPTION_INFORMATION`, null for a
        // dump of no fault, valid for the call.
        if let Some(exception) = unsafe { exception.as_ref() }
            && (exception.client_pointers == 0 || ours)
            // SAFETY: the pointers are this process's own (`client_pointers`
            // is 0, or the dump is of this process, checked first) and name
            // the fault whose thread waits in it while its handler writes
            // the dump.
            && unsafe { fault_code(exception.pointers) }.is_some_and(fatal)
        {
            let (pointers, thread) = (exception.pointers, exception.thread);
            // SAFETY: the fault's pointers in this process, of the thread
            // the dump names, which waits in the fault while its handler
            // writes the dump.
            unsafe { report(pointers, thread) };
        }
        onward.call()
    }
}

/// Links the report on the game's crash dumps: `MiniDumpWriteDump` of the
/// system's `dbghelp.dll`, which a `dbghelp.dll` proxy in the game's folder
/// forwards to.
pub(crate) fn link(loader: &Loader) {
    let found = crate::engine::module::Module::system("dbghelp.dll")
        .and_then(|m| m.export("MiniDumpWriteDump"));
    let target = Target::Found(
        "dbghelp.MiniDumpWriteDump",
        found.map(|f| f as *const () as u64),
    );
    loader.link_feature(
        "crash blame on crash dumps (MiniDumpWriteDump)",
        target,
        &DUMP_NEXT,
        dump,
        HookFlags::NONE,
    );
}

/// The address range of a loaded module, `None` when Windows does not say.
pub(crate) fn range_of(module: HMODULE) -> Option<(usize, usize)> {
    let mut info = MODULEINFO {
        lpBaseOfDll: core::ptr::null_mut(),
        SizeOfImage: 0,
        EntryPoint: core::ptr::null_mut(),
    };
    // SAFETY: `GetCurrentProcess` takes no arguments and has no
    // preconditions; the pseudo handle needs no closing.
    let process = unsafe { GetCurrentProcess() };
    // SAFETY: `process` is this process's pseudo handle, `module` the
    // caller's loaded module, and `info` a writable `MODULEINFO` of the size
    // passed.
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

/// The base of the module `at` lies in and its file name, written into
/// `name` with bytes outside printable ASCII as `?`. `None` outside a mapped
/// image, or when Windows does not name the file in [`MAPPED_PATH`] units.
///
/// Allocates nothing and takes no loader lock, which another thread of the
/// dying process may hold, so the report may call it: `VirtualQuery` and
/// `GetMappedFileNameW` only ask the kernel about the address
/// (`NtQueryVirtualMemory`, in the `kernelbase` of Wine and the `psapi` of `ReactOS`;
/// Windows does not document it).
fn module_at(at: usize, name: &mut [u8; NAME]) -> Option<(usize, &[u8])> {
    let region = crate::engine::pages::query(at).filter(|r| r.kind == MEM_IMAGE)?;
    let mut path = [0u16; MAPPED_PATH];
    // SAFETY: `GetCurrentProcess` takes no arguments and has no
    // preconditions; the pseudo handle needs no closing.
    let process = unsafe { GetCurrentProcess() };
    // SAFETY: `GetMappedFileNameW` reads no memory at the base, which it
    // only looks up, and writes at most `MAPPED_PATH` units into `path`.
    let len = unsafe {
        GetMappedFileNameW(
            process,
            core::ptr::with_exposed_provenance(region.allocation),
            path.as_mut_ptr(),
            u32::try_from(MAPPED_PATH).unwrap_or(0),
        )
    } as usize;
    // A full buffer may hold a cut path, whose last part is no file name.
    let path = path.get(..len).filter(|_| len < MAPPED_PATH)?;
    // A Windows path ends in `\name`; one under Wine may end in `/name`.
    let file = path
        .rsplit(|&unit| unit == u16::from(b'\\') || unit == u16::from(b'/'))
        .next()
        .filter(|file| !file.is_empty())?;
    let file = &file[..file.len().min(NAME)];
    for (byte, &unit) in name.iter_mut().zip(file) {
        *byte = u8::try_from(unit)
            .ok()
            .filter(|byte| byte.is_ascii_graphic() || *byte == b' ')
            .unwrap_or(b'?');
    }
    Some((region.allocation, &name[..file.len()]))
}

/// Opens `path` for the filter, `access` as given; 0 when it cannot.
fn open(path: &Path, access: u32) -> usize {
    let wide: Vec<u16> = wide(path).into_iter().chain(Some(0)).collect();
    open_wide(&wide, access)
}

/// `path` in UTF-16, without a NUL.
fn wide(path: &Path) -> Vec<u16> {
    path.as_os_str().to_string_lossy().encode_utf16().collect()
}

/// Opens the NUL-terminated UTF-16 path `wide`, `access` as given; 0 when
/// it cannot. Allocates nothing, so the report may call it.
fn open_wide(wide: &[u16], access: u32) -> usize {
    if wide.last() != Some(&0) {
        return 0;
    }
    // SAFETY: `wide` is a NUL-terminated UTF-16 path alive for the call, and
    // the other arguments are plain values or null; the handle stays open for
    // the process.
    let handle = unsafe {
        CreateFileW(
            wide.as_ptr(),
            access,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            core::ptr::null(),
            OPEN_ALWAYS,
            FILE_ATTRIBUTE_NORMAL,
            core::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        0
    } else {
        handle.expose_provenance()
    }
}

/// Writes `prefix`, the crash's `time` as `YYYY-MM-DD_HH-MM-SS`, `.log` and
/// a NUL into `out`; the length written, or `None` when it does not fit.
/// Allocates nothing.
fn crash_file_name(prefix: &[u16], time: &SYSTEMTIME, out: &mut [u16]) -> Option<usize> {
    let mut at = 0;
    let mut put = |unit: u16| -> Option<()> {
        *out.get_mut(at)? = unit;
        at += 1;
        Some(())
    };
    for &unit in prefix {
        put(unit)?;
    }
    let fields = [
        (time.wYear, 4, u16::from(b'-')),
        (time.wMonth, 2, u16::from(b'-')),
        (time.wDay, 2, u16::from(b'_')),
        (time.wHour, 2, u16::from(b'-')),
        (time.wMinute, 2, u16::from(b'-')),
        (time.wSecond, 2, 0),
    ];
    for (value, digits, after) in fields {
        for place in (0..digits).rev() {
            put(u16::from(b'0') + value / 10u16.pow(place) % 10)?;
        }
        if after != 0 {
            put(after)?;
        }
    }
    for &byte in b".log\0" {
        put(u16::from(byte))?;
    }
    Some(at)
}

/// The last crash of the session before, if it crashed.
pub(crate) fn last() -> Option<&'static str> {
    STATE.last.get().map(String::as_str)
}

/// Starts blaming crashes on `modules`, `(index, id, start, end)`, writing
/// each crash to the loader's log file, to a crash file of its own in
/// `logs_dir` and to `enshwick/last-crash.txt` in `mods_dir`; reads the last
/// session's crash first and deletes the oldest crash files.
pub(crate) fn install(modules: &[(usize, String, usize, usize)], logs_dir: &Path, mods_dir: &Path) {
    let own = mods_dir.join(crate::engine::settings::LOADER_ID);
    let _made = std::fs::create_dir_all(&own);
    let last_path = own.join("last-crash.txt");
    if let Ok(text) = std::fs::read_to_string(&last_path)
        && !text.trim().is_empty()
    {
        let text = text.trim().to_owned();
        crate::log!(
            WARN,
            "{}: {text}",
            crate::engine::diag::crash_heading(&text)
        );
        let _first = STATE.last.set(text);
    }
    let _cleared = std::fs::write(&last_path, "");
    crate::engine::log::prune(logs_dir, CRASH_SOURCE, KEPT_CRASHES);
    let ranges = modules
        .iter()
        .map(|(owner, id, start, end)| Range::new(*start, *end, ModIndex::new(*owner), id))
        .collect();
    let state = Setup {
        ranges,
        log: crate::engine::log::loader_file().map_or(0, |path| open(path, FILE_APPEND_DATA)),
        crash_file: wide(&logs_dir.join(format!("{CRASH_SOURCE}_"))).into_boxed_slice(),
        last: open(&last_path, GENERIC_READ | GENERIC_WRITE),
    };
    if STATE.setup.set(state).is_err() {
        return;
    }
    // SAFETY: `first_chance` has the `PVECTORED_EXCEPTION_HANDLER` signature,
    // lives for the process and never unwinds.
    if unsafe { AddVectoredExceptionHandler(1, Some(first_chance)) }.is_null() {
        crate::log!(
            WARN,
            "crash blame names no callbacks: no first-chance handler"
        );
    }
    reassert();
}

/// Puts the loader's filter back on top when another took its place, and
/// calls that one after it.
pub(crate) fn reassert() {
    if STATE.setup.get().is_none() {
        return;
    }
    // SAFETY: `filter` has the `LPTOP_LEVEL_EXCEPTION_FILTER` signature,
    // lives for the process and never unwinds.
    let previous = unsafe { SetUnhandledExceptionFilter(Some(filter)) };
    let ours = filter as *const () as usize;
    if let Some(previous) = previous
        && previous as *const () as usize != ours
    {
        let next = previous as *const () as usize;
        if STATE.next.swap(next, Ordering::AcqRel) != next {
            crate::log!(
                INFO,
                "crash blame runs before the crash filter at {next:#x}, which another module set"
            );
        }
    }
}

/// Writes the report of the fault `pointers` names, once per process.
///
/// # Safety
///
/// - `pointers` is null or points at the fault's valid
///   `EXCEPTION_POINTERS` in this process, whose records are null or valid.
/// - The thread `thread` is stopped in the fault or is the one running this.
unsafe fn report(pointers: *const EXCEPTION_POINTERS, thread: u32) {
    if STATE.inside.swap(true, Ordering::AcqRel) {
        return;
    }
    let Some(state) = STATE.setup.get() else {
        return;
    };
    // SAFETY: the caller's contract makes `pointers` null or the fault's
    // valid `EXCEPTION_POINTERS`.
    let Some(pointers) = (unsafe { pointers.as_ref() }) else {
        return;
    };
    // SAFETY: this thread's `inside.swap(true)` returned false, which one
    // thread in the process ever sees, so nothing else borrows the line.
    let line = unsafe { &mut *STATE.line.0.get() };
    line.len = 0;
    // SAFETY: the caller's contract, passed on: the fault's valid pointers,
    // and `thread` stopped in the fault or running this.
    unsafe { record(state, pointers, thread, line) };
    write(state.log, line.as_bytes());
    let mut time = SYSTEMTIME {
        wYear: 0,
        wMonth: 0,
        wDayOfWeek: 0,
        wDay: 0,
        wHour: 0,
        wMinute: 0,
        wSecond: 0,
        wMilliseconds: 0,
    };
    // SAFETY: `time` is a writable `SYSTEMTIME`.
    unsafe { GetSystemTime(&raw mut time) };
    let mut name = [0u16; CRASH_PATH];
    if let Some(len) = crash_file_name(&state.crash_file, &time, &mut name) {
        write(open_wide(&name[..len], FILE_APPEND_DATA), line.as_bytes());
    }
    rewind(state.last);
    write(state.last, line.as_bytes());
}

unsafe extern "system" fn filter(pointers: *const EXCEPTION_POINTERS) -> i32 {
    // SAFETY: `GetCurrentThreadId` takes no arguments and has no
    // preconditions.
    let thread = unsafe { GetCurrentThreadId() };
    if STATE.handed_on.load(Ordering::Acquire) == thread {
        return CONTINUE_SEARCH;
    }
    // SAFETY: Windows calls an unhandled exception filter on the faulting
    // thread with null or the fault's valid `EXCEPTION_POINTERS`.
    unsafe { report(pointers, thread) };
    STATE.handed_on.store(thread, Ordering::Release);
    let next = STATE.next.load(Ordering::Acquire);
    if next == 0 {
        return CONTINUE_SEARCH;
    }
    // SAFETY: the filter the process had before the loader's, as Windows
    // handed it back; it takes the same pointers.
    let next: LPTOP_LEVEL_EXCEPTION_FILTER = unsafe { core::mem::transmute::<usize, _>(next) };
    match next {
        // SAFETY: `next` is the filter the process had before the loader's,
        // as `SetUnhandledExceptionFilter` handed it back, called on the
        // faulting thread with the fault's own pointers.
        Some(next) => unsafe { next(pointers) },
        None => CONTINUE_SEARCH,
    }
}

/// Describes the fault `pointers` names into `line`.
///
/// # Safety
///
/// - `pointers` are the fault's, with records null or valid for the call.
/// - The thread `thread` is stopped in the fault or is the one running this.
unsafe fn record(state: &Setup, pointers: &EXCEPTION_POINTERS, thread: u32, line: &mut Line) {
    // SAFETY: the caller's contract makes both records null or valid for the
    // call.
    let (Some(record), Some(context)) = (unsafe { pointers.ExceptionRecord.as_ref() }, unsafe {
        pointers.ContextRecord.as_ref()
    }) else {
        return;
    };
    let code: NTSTATUS = record.ExceptionCode;
    let at = record.ExceptionAddress.addr();
    let mut frames = [0usize; FRAMES];
    // A stack overflow leaves no room to unwind.
    let found = if code == EXCEPTION_STACK_OVERFLOW {
        0
    } else {
        // SAFETY: `context` is the faulting thread's, which by the caller's
        // contract is stopped in the fault or runs this.
        unsafe { unwind(context, &mut frames) }
    };
    let walked = code != EXCEPTION_STACK_OVERFLOW && found < FRAMES;
    let callback = noted(thread, at).map(|(owner, f)| (owner, f.name()));
    let mut name = [0; NAME];
    let module = if names_module(&state.ranges, at, code) {
        module_at(at, &mut name)
    } else {
        None
    };
    let blame = settle(
        blame(&state.ranges, at, callback, &frames[..found]),
        walked,
        module.is_some(),
    );
    let access = (code == EXCEPTION_ACCESS_VIOLATION && record.NumberParameters >= 2).then(|| {
        (
            record.ExceptionInformation[0] as u64,
            record.ExceptionInformation[1] as u64,
        )
    });
    let mut time = SYSTEMTIME {
        wYear: 0,
        wMonth: 0,
        wDayOfWeek: 0,
        wDay: 0,
        wHour: 0,
        wMinute: 0,
        wSecond: 0,
        wMilliseconds: 0,
    };
    // SAFETY: `time` is a writable `SYSTEMTIME`.
    unsafe { GetSystemTime(&raw mut time) };
    describe(
        line,
        [
            time.wYear,
            time.wMonth,
            time.wDay,
            time.wHour,
            time.wMinute,
            time.wSecond,
        ],
        code.cast_unsigned(),
        at,
        module,
        access,
        blame,
        &state.ranges,
    );
}

/// The return addresses the crash walk finds on the calling thread's stack,
/// innermost first, for tests of code it must pass.
#[cfg(test)]
#[inline(never)]
pub(crate) fn walk_here() -> Vec<usize> {
    // SAFETY: an all-zero context is a valid value to capture into.
    let mut aligned = crate::engine::detour::ThreadContext(unsafe { core::mem::zeroed() });
    // SAFETY: a writable context on its 16-byte boundary.
    unsafe {
        windows_sys::Win32::System::Diagnostics::Debug::RtlCaptureContext(&raw mut aligned.0);
    }
    walk_from(&aligned.0)
}

/// The return addresses the crash walk finds above a frame stopped as
/// `context` says, over a stack the caller keeps unchanged, for tests of
/// code it must pass.
#[cfg(test)]
pub(crate) fn walk_from(
    context: &windows_sys::Win32::System::Diagnostics::Debug::CONTEXT,
) -> Vec<usize> {
    let mut frames = [0usize; FRAMES];
    // SAFETY: the walk reads only committed, readable memory, and the
    // caller keeps the stack it names unchanged during it.
    let found = unsafe { unwind(context, &mut frames) };
    frames[..found].to_vec()
}

#[cfg(test)]
mod tests;
mod unwind;
