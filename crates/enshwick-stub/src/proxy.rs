//! Forwards every POWRPROF.dll export to the system copy.
//!
//! Each export is a stub that jumps through `FORWARDS`. `install` fills the
//! table before the game can call anything; an export the system copy lacks
//! (an older Windows, or Wine's builtin) returns `ERROR_CALL_NOT_IMPLEMENTED`.

use core::sync::atomic::{AtomicUsize, Ordering};

use windows_sys::Win32::Foundation::ERROR_CALL_NOT_IMPLEMENTED;
use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
use windows_sys::Win32::System::SystemInformation::GetSystemDirectoryW;

include!(concat!(env!("OUT_DIR"), "/stubs.rs"));

/// The address each export stub jumps to, in export order. `AtomicUsize` has
/// the layout of `usize`, which the stubs read directly.
pub(crate) static FORWARDS: [AtomicUsize; EXPORTS.len()] =
    [const { AtomicUsize::new(0) }; EXPORTS.len()];

extern "system" fn not_implemented() -> u32 {
    ERROR_CALL_NOT_IMPLEMENTED
}

/// Loads the system POWRPROF.dll and points every stub at it. Returns how many
/// exports it lacks, or `None` when it cannot be loaded at all.
pub(crate) fn install() -> Option<u32> {
    let fallback = (not_implemented as *const ()).addr();
    for forward in &FORWARDS {
        forward.store(fallback, Ordering::Release);
    }

    let mut path = [0u16; 512];
    #[expect(clippy::cast_possible_truncation, reason = "the buffer is 512 units")]
    // SAFETY: the buffer and its length match.
    let len = unsafe { GetSystemDirectoryW(path.as_mut_ptr(), path.len() as u32) } as usize;
    let suffix: Vec<u16> = r"\powrprof.dll".encode_utf16().chain([0]).collect();
    let end = len
        .checked_add(suffix.len())
        .filter(|&end| len != 0 && end <= path.len())?;
    path[len..end].copy_from_slice(&suffix);
    // SAFETY: `path` is NUL-terminated.
    let module = unsafe { LoadLibraryW(path.as_ptr()) };
    if module.is_null() {
        return None;
    }

    let mut missing = 0;
    for (forward, name) in FORWARDS.iter().zip(EXPORTS) {
        // SAFETY: `module` is loaded and `name` is NUL-terminated.
        match unsafe { GetProcAddress(module, name.as_ptr()) } {
            Some(function) => forward.store((function as *const ()).addr(), Ordering::Release),
            None => missing += 1,
        }
    }
    Some(missing)
}
