//! Holds the dedicated server's own code until the loader has armed its hooks.
//!
//! The server's task system and other start-up code run a few milliseconds
//! after the process starts, before the loader, on its own thread, could arm
//! a hook on them. While Windows loads the stub as one of the server's
//! imports, the server's entry point has not run yet. The stub points that
//! entry at [`held_entry`], which waits until the loader calls [`release`],
//! or [`LONGEST_HOLD`] passes, puts the entry's bytes back and runs it. The
//! loader finds its entries well within the hold: on the 1.0 server, by
//! patterns alone, in about 15 ms, and 112 ms on a single core.
//!
//! The client starts under Steam's DRM wrapper, whose entry point the stub
//! leaves alone.

use core::ffi::c_void;
use core::sync::atomic::{AtomicBool, AtomicPtr, AtomicUsize, Ordering};
use std::sync::OnceLock;

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, WAIT_TIMEOUT};
use windows_sys::Win32::System::Diagnostics::Debug::FlushInstructionCache;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Memory::{PAGE_EXECUTE_READWRITE, VirtualProtect};
use windows_sys::Win32::System::Threading::{
    CreateEventW, ExitProcess, GetCurrentProcess, SetEvent, TerminateProcess, WaitForSingleObject,
};

/// The longest the server's code waits for the loader, in milliseconds.
const LONGEST_HOLD: u32 = 10_000;
/// `jmp [rip+0]` followed by the absolute target.
const JUMP: usize = 14;

/// The event [`release`] sets; null when nothing is held.
static EVENT: AtomicPtr<c_void> = AtomicPtr::new(core::ptr::null_mut());
/// Whether the hold ran out before [`release`].
static EXPIRED: AtomicBool = AtomicBool::new(false);
/// The server's entry point.
static ENTRY: AtomicUsize = AtomicUsize::new(0);
/// The entry's own first bytes.
static SAVED: OnceLock<[u8; JUMP]> = OnceLock::new();

/// The process entry point: `mainCRTStartup(peb)`.
type Entry = unsafe extern "system" fn(peb: *mut c_void) -> u32;

/// Whether the process is the dedicated server, by the file name of its
/// executable.
fn is_server() -> bool {
    std::env::current_exe().is_ok_and(|exe| {
        exe.file_stem()
            .is_some_and(|stem| stem.eq_ignore_ascii_case("enshrouded_server"))
    })
}

/// The server's entry point, from its PE header.
fn entry_point() -> Option<usize> {
    // SAFETY: `GetModuleHandleW` takes a null name as the executable, loaded
    // for the life of the process; it has no other preconditions.
    let base = unsafe { GetModuleHandleW(core::ptr::null()) };
    if base.is_null() {
        return None;
    }
    let base = base.cast::<u8>();
    // SAFETY: `base` is non-null (`is_null` returns first) and the mapped
    // executable, whose DOS header, at least 0x40 bytes at the start of every
    // PE image, holds `e_lfanew` at 0x3c.
    let field = unsafe { base.add(0x3c) };
    // SAFETY: `field` is `e_lfanew` inside the mapped DOS header, four
    // readable bytes nothing writes; `read_unaligned` needs no alignment.
    // The stub links no API crate, so it reads the image's headers itself.
    // ast-grep-ignore: game-memory-through-raw
    let nt = unsafe { field.cast::<u32>().read_unaligned() };
    let nt = usize::try_from(nt).ok()?;
    // SAFETY: `e_lfanew` is the offset of the NT headers inside the mapped
    // image, and `AddressOfEntryPoint` is at 0x28 in the NT headers of a
    // PE32+ image, inside its mapped headers.
    let field = unsafe { base.add(nt + 0x28) };
    // SAFETY: `field` is `AddressOfEntryPoint` inside the mapped NT headers,
    // four readable bytes nothing writes; `read_unaligned` needs no
    // alignment. The stub links no API crate, so it reads the headers itself.
    // ast-grep-ignore: game-memory-through-raw
    let rva = unsafe { field.cast::<u32>().read_unaligned() };
    let rva = usize::try_from(rva).ok().filter(|&rva| rva != 0)?;
    Some(base.addr() + rva)
}

/// Writes `bytes` over code at `at`.
///
/// # Safety
///
/// - `at` is the start of `bytes.len()` bytes of code mapped in the
///   executable's image.
/// - No thread runs or reads those bytes while they change.
unsafe fn write_code(at: usize, bytes: &[u8]) -> bool {
    let target = core::ptr::with_exposed_provenance_mut::<u8>(at);
    let mut old = 0u32;
    // SAFETY: `target` starts `bytes.len()` bytes mapped in the executable's
    // image (the caller's contract), and `old` is a writable `u32` on this
    // stack.
    if unsafe {
        VirtualProtect(
            target.cast(),
            bytes.len(),
            PAGE_EXECUTE_READWRITE,
            &raw mut old,
        )
    } == 0
    {
        return false;
    }
    // SAFETY: `VirtualProtect` made the `bytes.len()` bytes at `target`
    // writable (a failure returns first), no thread runs or reads them (the
    // caller's contract), and `bytes` is a separate borrowed slice.
    unsafe { core::ptr::copy_nonoverlapping(bytes.as_ptr(), target, bytes.len()) };
    // SAFETY: `target` starts `bytes.len()` bytes mapped in the executable's
    // image (the caller's contract); `old` is the protection they had and a
    // writable `u32` on this stack.
    let _restored = unsafe { VirtualProtect(target.cast(), bytes.len(), old, &raw mut old) };
    // SAFETY: `GetCurrentProcess` has no preconditions; its pseudo handle
    // needs no closing.
    let process = unsafe { GetCurrentProcess() };
    // SAFETY: `process` is the current process's pseudo handle and `target`
    // the `bytes.len()` mapped bytes just written.
    let _flushed = unsafe { FlushInstructionCache(process, target.cast(), bytes.len()) };
    true
}

/// On the dedicated server, points its entry at [`held_entry`]. Called from
/// `DllMain` while Windows loads the server's imports, before its entry runs.
pub(crate) fn hold() {
    if !is_server() {
        return;
    }
    let Some(entry) = entry_point() else {
        return;
    };
    // SAFETY: `CreateEventW` takes null attributes and a null name; it makes
    // a manual-reset event, not set.
    let event: HANDLE = unsafe { CreateEventW(core::ptr::null(), 1, 0, core::ptr::null()) };
    if event.is_null() {
        return;
    }
    let mut saved = [0u8; JUMP];
    // SAFETY: `entry` is the executable's entry point from its PE header, in
    // its mapped code, which runs past `JUMP` bytes; nothing writes it while
    // Windows loads the imports.
    unsafe {
        core::ptr::copy_nonoverlapping(
            core::ptr::with_exposed_provenance::<u8>(entry),
            saved.as_mut_ptr(),
            JUMP,
        );
    };
    if SAVED.set(saved).is_err() {
        return;
    }
    ENTRY.store(entry, Ordering::Relaxed);
    EVENT.store(event, Ordering::Release);
    let mut jump = [0u8; JUMP];
    jump[..6].copy_from_slice(&[0xFF, 0x25, 0, 0, 0, 0]);
    jump[6..].copy_from_slice(&(held_entry as *const () as usize).to_ne_bytes());
    // SAFETY: `entry` starts `JUMP` bytes of the executable's mapped code
    // (the `saved` copy read them); it has not run and no other thread does
    // yet, since Windows runs it after every import's `DllMain`, this one
    // included.
    if !unsafe { write_code(entry, &jump) } {
        EVENT.store(core::ptr::null_mut(), Ordering::Release);
        // SAFETY: `event` is the handle `CreateEventW` returned in this call,
        // no longer published in `EVENT`, closed once here.
        let _closed = unsafe { CloseHandle(event) };
    }
}

/// Lets the server's code run. Called by the loader once its hooks are
/// armed, and by the stub when the loader returns: 1 when the code was held
/// until now, 2 when the hold ran out before, 0 when nothing was held.
pub(crate) extern "system" fn release() -> u32 {
    let event = EVENT.load(Ordering::Acquire);
    if event.is_null() {
        return 0;
    }
    // SAFETY: `event` is non-null (`is_null` returns 0 first), so the handle
    // `hold` created and published, which is never closed while published.
    let set = unsafe { SetEvent(event) } != 0;
    if EXPIRED.load(Ordering::Acquire) {
        2
    } else {
        u32::from(set)
    }
}

/// Runs in place of the server's entry point: waits for [`release`], puts
/// the entry's bytes back and runs it.
///
/// # Safety
///
/// - Called by Windows as the process entry point, once, on the process's
///   first thread, with its `peb`.
/// - `hold` published `EVENT`, `ENTRY` and `SAVED` before pointing the entry
///   here.
unsafe extern "system" fn held_entry(peb: *mut c_void) -> u32 {
    let event = EVENT.load(Ordering::Acquire);
    // SAFETY: `event` is the handle `hold` published before pointing the
    // entry here (the caller's contract), never closed while published.
    if unsafe { WaitForSingleObject(event, LONGEST_HOLD) } == WAIT_TIMEOUT {
        EXPIRED.store(true, Ordering::Release);
    }
    let entry = ENTRY.load(Ordering::Relaxed);
    let restored = SAVED.get().is_some_and(|saved| {
        // SAFETY: `entry` starts the `JUMP` bytes of mapped code `hold`
        // saved and overwrote (the caller's contract); only this thread runs
        // the entry, and it has not run the real one yet.
        unsafe { write_code(entry, saved) }
    });
    if !restored {
        // The entry still jumps here: running it would recurse until the
        // stack overflows.
        crate::note::note(&format!(
            "the server's own start could not be put back ({}); the server stops",
            std::io::Error::last_os_error()
        ));
        // SAFETY: `GetCurrentProcess` has no preconditions; its pseudo
        // handle needs no closing.
        let process = unsafe { GetCurrentProcess() };
        // SAFETY: `process` is this process's pseudo handle, which has every
        // right on it. Terminating runs no DLL's detach code, which threads
        // stopped mid-start could not finish.
        let _ended = unsafe { TerminateProcess(process, 1) };
        // SAFETY: `ExitProcess` has no preconditions; it is reached only if
        // the termination did not take, and ends the process with code 1.
        unsafe { ExitProcess(1) };
    }
    // SAFETY: `entry` is the executable's entry point from its PE header
    // (the caller's contract), a function of the `Entry` signature.
    let entry: Entry = unsafe { core::mem::transmute(entry) };
    // SAFETY: the real entry point runs once, on the thread Windows started
    // for it, with the `peb` Windows passed (the caller's contract), and its
    // own bytes are back: a failed `write_code` ended the process above.
    unsafe { entry(peb) }
}
