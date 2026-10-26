//! Other proxy DLLs, such as another mod's own `powrprof.dll`, `dinput8.dll`,
//! `winmm.dll` or `dbghelp.dll`, placed in `mods\chain\` instead of the game's
//! folder: each is loaded on the stub's thread, in file name order, and its
//! `DllMain` runs before the Enshwick loader starts. That is later than from
//! the game's folder: the game keeps the imports it has already bound, on the
//! client its code runs meanwhile, and on the server the loads count against
//! the hold of its start. A later load by the same file name gets the chained
//! copy.

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt as _;
use std::path::{Path, PathBuf};

use windows_sys::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError};
use windows_sys::Win32::System::LibraryLoader::{LOAD_WITH_ALTERED_SEARCH_PATH, LoadLibraryExW};
use windows_sys::Win32::System::Threading::{CreateMutexW, GetCurrentProcessId};

/// The folder, under the game's, that holds the DLLs to chainload.
pub(crate) const FOLDER: &str = r"mods\chain";
/// The loader, under the game's folder.
pub(crate) const LOADER: &str = r"mods\enshwick\enshwick.dll";
/// The name of the mutex the first stub of a process makes, before its id.
const MUTEX_PREFIX: &str = r"Local\enshwick-stub-";

/// Whether this is the first copy of the stub to start in this process. A
/// copy of the stub among the chained DLLs, or a second proxy of ours under
/// another name, then forwards its exports and starts nothing. When the
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

/// The DLLs in `folder`, sorted by file name; none when it is missing.
pub(crate) fn files(folder: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("dll"))
        })
        .collect();
    files.sort_by_cached_key(|path| path.file_name().map(OsStr::to_ascii_lowercase));
    files
}

/// Loads every DLL of `mods\chain\` under `game_dir`; one that does not load
/// is skipped.
pub(crate) fn load(game_dir: &Path) {
    for path in files(&game_dir.join(FOLDER)) {
        let path = wide(path.as_os_str());
        // SAFETY: an absolute, NUL-terminated path, as the altered search
        // path needs to resolve the DLL's own dependencies beside it. This
        // thread holds no loader lock, since the stub's `DllMain` returned
        // before it started, so the DLL's `DllMain` takes the lock without
        // deadlocking.
        let _module = unsafe {
            LoadLibraryExW(
                path.as_ptr(),
                core::ptr::null_mut(),
                LOAD_WITH_ALTERED_SEARCH_PATH,
            )
        };
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

    /// A folder of the system's temporary folder, removed when dropped.
    struct Scratch(PathBuf);

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _removed = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn the_chain_is_every_dll_in_the_folder_in_name_order_ignoring_case() {
        let folder =
            Scratch(std::env::temp_dir().join(format!("enshwick-chain-{}", std::process::id())));
        std::fs::create_dir_all(folder.0.join("nested.dll")).unwrap();
        for file in ["winmm.dll", "Beta.DLL", "alpha.dll", "readme.txt"] {
            std::fs::write(folder.0.join(file), b"").unwrap();
        }
        let names: Vec<String> = files(&folder.0)
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["alpha.dll", "Beta.DLL", "winmm.dll"]);
    }

    #[test]
    fn a_missing_folder_chains_nothing() {
        assert!(files(Path::new(r"C:\no\such\folder")).is_empty());
    }
}
