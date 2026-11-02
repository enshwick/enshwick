//! Loading the Enshwick loader once per process, with a safe DLL search.

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt as _;
use std::path::Path;

use windows_sys::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError, HMODULE};
use windows_sys::Win32::System::LibraryLoader::{
    LOAD_LIBRARY_SEARCH_DEFAULT_DIRS, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR, LoadLibraryExW,
};
use windows_sys::Win32::System::Threading::{CreateMutexW, GetCurrentProcessId};

/// The loader, under the game's folder.
pub(crate) const LOADER: &str = r"mods\enshwick\enshwick.dll";
/// The name of the mutex the first stub of a process makes, before its id.
const MUTEX_PREFIX: &str = r"Local\enshwick-stub-";

/// Whether this is the first copy of the stub to start in this process. A
/// second proxy of ours under another name then forwards its exports and starts nothing. When the
/// mutex cannot be made, no copy counts as first and the game runs vanilla:
/// the loader never starts twice.
pub(crate) fn first_in_process() -> bool {
    // SAFETY: no preconditions.
    let pid = unsafe { GetCurrentProcessId() };
    first_holder(&format!("{MUTEX_PREFIX}{pid}"))
}

/// Whether this call made the named mutex, which stays open for the
/// process's life.
fn first_holder(name: &str) -> bool {
    let name = wide(OsStr::new(name));
    // SAFETY: default security, not owned, a NUL-terminated name.
    let mutex = unsafe { CreateMutexW(core::ptr::null(), 0, name.as_ptr()) };
    // SAFETY: no preconditions; read right after the call that set it.
    let error = unsafe { GetLastError() };
    !mutex.is_null() && error != ERROR_ALREADY_EXISTS
}

/// Loads the DLL at `path`, an absolute path with `\` separators, with its
/// imports resolved only from its own folder, the game's folder and the
/// system folder, never from the current directory or `PATH`; null when it
/// does not load, with the error in `GetLastError`.
pub(crate) fn load_library(path: &Path) -> HMODULE {
    let path = wide(path.as_os_str());
    // SAFETY: `path` is NUL-terminated and lives across the call. This
    // thread holds no loader lock, since the stub's `DllMain` returned
    // before it started, so the DLL's `DllMain` takes the lock without
    // deadlocking.
    unsafe {
        LoadLibraryExW(
            path.as_ptr(),
            core::ptr::null_mut(),
            LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_DEFAULT_DIRS,
        )
    }
}

/// `text` as UTF-16, NUL-terminated.
pub(crate) fn wide(text: &OsStr) -> Vec<u16> {
    text.encode_wide().chain([0]).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_first_holder_of_a_name_starts() {
        let name = format!("Local\\enshwick-stub-test-{}", std::process::id());
        assert!(first_holder(&name));
        assert!(!first_holder(&name));
    }
}
