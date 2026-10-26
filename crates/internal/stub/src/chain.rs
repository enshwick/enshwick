//! Other proxy DLLs, such as another mod's own `powrprof.dll`, `dinput8.dll`,
//! `winmm.dll` or `dbghelp.dll`: placed in `mods\chain\` instead of the game's
//! folder, each is loaded before the Enshwick loader, in file name order, so
//! it runs as it would from the game's folder. Their exports are not used:
//! the game calls the system copies, and this stub forwards `powrprof`.

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt as _;
use std::path::{Path, PathBuf};

use windows_sys::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError};
use windows_sys::Win32::System::LibraryLoader::{LOAD_WITH_ALTERED_SEARCH_PATH, LoadLibraryExW};
use windows_sys::Win32::System::Threading::{CreateMutexW, GetCurrentProcessId};

/// The folder, under the game's, that holds the DLLs to chainload.
pub(crate) const FOLDER: &str = r"mods\chain";

/// Whether this is the first copy of the stub to start in this process. A
/// copy of the stub among the chained DLLs, or a second proxy of ours under
/// another name, then forwards its exports and starts nothing.
pub(crate) fn first_in_process() -> bool {
    // SAFETY: no preconditions.
    let pid = unsafe { GetCurrentProcessId() };
    first_holder(&format!("Local\\enshwick-stub-{pid}"))
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
    files.sort_by_key(|path| path.file_name().map(OsStr::to_ascii_lowercase));
    files
}

/// Loads every DLL of `mods\chain\` under `game_dir`; one that does not load
/// is skipped.
pub(crate) fn load(game_dir: &Path) {
    for path in files(&game_dir.join(FOLDER)) {
        let path = wide(path.as_os_str());
        // SAFETY: a NUL-terminated path; the altered search path lets the
        // DLL's own dependencies resolve beside it. Its `DllMain` runs here,
        // on the stub's thread, outside the loader lock.
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
fn wide(text: &OsStr) -> Vec<u16> {
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

    #[test]
    fn the_chain_is_every_dll_in_the_folder_in_name_order() {
        let folder = std::env::temp_dir().join(format!("enshwick-chain-{}", std::process::id()));
        std::fs::create_dir_all(folder.join("nested.dll")).unwrap();
        for file in ["winmm.dll", "Dinput8.DLL", "powrprof.dll", "readme.txt"] {
            std::fs::write(folder.join(file), b"").unwrap();
        }
        let names: Vec<String> = files(&folder)
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        std::fs::remove_dir_all(&folder).unwrap();
        assert_eq!(names, ["Dinput8.DLL", "powrprof.dll", "winmm.dll"]);
    }

    #[test]
    fn a_missing_folder_chains_nothing() {
        assert!(files(Path::new(r"C:\no\such\folder")).is_empty());
    }
}
