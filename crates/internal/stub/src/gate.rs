//! Holds the dedicated server's own code until the loader has armed its hooks.
//!
//! The server's task system and other start-up code run a few milliseconds
//! after the process starts, before the loader, on its own thread, could arm
//! a hook on them. While Windows loads the stub as one of the server's
//! imports, the server's entry point has not run yet. The stub points that
//! entry at [`held_entry`], which waits until the loader calls [`release`],
//! or [`LONGEST_HOLD`] passes, puts the entry's bytes back and runs it.
//!
//! The client starts under Steam's DRM wrapper, whose entry point the stub
//! leaves alone.

use core::ffi::c_void;
use core::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};
use std::sync::OnceLock;

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::System::Diagnostics::Debug::FlushInstructionCache;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Memory::{PAGE_EXECUTE_READWRITE, VirtualProtect};
use windows_sys::Win32::System::Threading::{
    CreateEventW, GetCurrentProcess, SetEvent, WaitForSingleObject,
};

/// The longest the server's code waits for the loader, in milliseconds.
const LONGEST_HOLD: u32 = 10_000;
/// `jmp [rip+0]` followed by the absolute target.
const JUMP: usize = 14;

/// The event [`release`] sets; null when nothing is held.
static EVENT: AtomicPtr<c_void> = AtomicPtr::new(core::ptr::null_mut());
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
    // SAFETY: a null name is the executable, loaded for the life of the
    // process.
    let base = unsafe { GetModuleHandleW(core::ptr::null()) };
    if base.is_null() {
        return None;
    }
    let base = base.cast::<u8>();
    // SAFETY: every PE image starts with a DOS header whose `e_lfanew`, at
    // 0x3c, points at the NT headers.
    let field = unsafe { base.add(0x3c) };
    // SAFETY: as above.
    let nt = unsafe { field.cast::<u32>().read_unaligned() };
    let nt = usize::try_from(nt).ok()?;
    // SAFETY: `AddressOfEntryPoint` is at 0x28 in the NT headers of a PE32+
    // image.
    let field = unsafe { base.add(nt + 0x28) };
    // SAFETY: as above.
    let rva = unsafe { field.cast::<u32>().read_unaligned() };
    let rva = usize::try_from(rva).ok().filter(|&rva| rva != 0)?;
    Some(base.addr() + rva)
}

/// Writes `bytes` over code at `at`.
///
/// # Safety
///
/// `at` is code no thread runs while it changes, `bytes.len()` long.
unsafe fn write_code(at: usize, bytes: &[u8]) -> bool {
    let target = core::ptr::with_exposed_provenance_mut::<u8>(at);
    let mut old = 0u32;
    // SAFETY: the range is inside the mapped executable.
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
    // SAFETY: the range is writable now and no thread runs it.
    unsafe { core::ptr::copy_nonoverlapping(bytes.as_ptr(), target, bytes.len()) };
    // SAFETY: as above; the old protection is put back.
    let _restored = unsafe { VirtualProtect(target.cast(), bytes.len(), old, &raw mut old) };
    // SAFETY: a pseudo handle that needs no closing.
    let process = unsafe { GetCurrentProcess() };
    // SAFETY: the current process and the range just written.
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
    // SAFETY: no name and no attributes; a manual-reset event, not set.
    let event: HANDLE = unsafe { CreateEventW(core::ptr::null(), 1, 0, core::ptr::null()) };
    if event.is_null() {
        return;
    }
    let mut saved = [0u8; JUMP];
    // SAFETY: the entry point's code is mapped and readable.
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
    // SAFETY: the entry has not run: Windows runs it after every import's
    // `DllMain`.
    if !unsafe { write_code(entry, &jump) } {
        EVENT.store(core::ptr::null_mut(), Ordering::Release);
        // SAFETY: the event created above, closed once.
        let _closed = unsafe { CloseHandle(event) };
    }
}

/// Lets the server's code run; true when it was held. Called by the loader
/// once its hooks are armed, and by the stub when the loader returns.
pub(crate) extern "system" fn release() -> u32 {
    let event = EVENT.load(Ordering::Acquire);
    if event.is_null() {
        return 0;
    }
    // SAFETY: the event `hold` created; it is never closed while held.
    u32::from(unsafe { SetEvent(event) } != 0)
}

/// Runs in place of the server's entry point: waits for [`release`], puts
/// the entry's bytes back and runs it.
///
/// # Safety
///
/// Called by Windows as the process entry point, once.
unsafe extern "system" fn held_entry(peb: *mut c_void) -> u32 {
    let event = EVENT.load(Ordering::Acquire);
    // SAFETY: the event `hold` created.
    let _waited = unsafe { WaitForSingleObject(event, LONGEST_HOLD) };
    let entry = ENTRY.load(Ordering::Relaxed);
    if let Some(saved) = SAVED.get() {
        // SAFETY: only this thread runs the entry, and it has not yet.
        let _written = unsafe { write_code(entry, saved) };
    }
    // SAFETY: the server's own entry point, its bytes restored.
    let entry: Entry = unsafe { core::mem::transmute(entry) };
    // SAFETY: called as Windows would have, with the same argument.
    unsafe { entry(peb) }
}
