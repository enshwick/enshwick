//! Modules of the process and their exports, found by name.

use std::ffi::{OsStr, OsString};
use std::os::windows::ffi::{OsStrExt as _, OsStringExt as _};
use std::path::{Path, PathBuf};

use windows_sys::Win32::Foundation::HMODULE;
use windows_sys::Win32::System::LibraryLoader::{
    AddDllDirectory, GET_MODULE_HANDLE_EX_FLAG_PIN, GetModuleHandleExW, GetProcAddress,
    LOAD_LIBRARY_SEARCH_DEFAULT_DIRS, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR, LoadLibraryExW,
    RemoveDllDirectory,
};
use windows_sys::Win32::System::SystemInformation::GetSystemDirectoryW;

/// Loads the DLL at `path` and keeps it, with its imports resolved only from
/// its own folder, `beside` (the original's folder of a dev reload copy), the
/// game's folder and the system folder, never from the current directory or
/// `PATH`.
///
/// # Errors
///
/// When `path` or `beside` has no absolute form, or the DLL or one of its
/// imports does not load; the error is the system's.
pub(crate) fn load_library(path: &Path, beside: Option<&Path>) -> std::io::Result<HMODULE> {
    // The folder flag takes only a full path; `absolute` also turns `/` into
    // `\`, which the system loader needs.
    let path = wide(std::path::absolute(path)?.as_os_str());
    let added = match beside {
        Some(dir) => {
            let dir = wide(std::path::absolute(dir)?.as_os_str());
            // SAFETY: `dir` is a live NUL-terminated absolute path; the cookie
            // is removed below once the load is done.
            let cookie = unsafe { AddDllDirectory(dir.as_ptr()) };
            if cookie.is_null() {
                return Err(std::io::Error::last_os_error());
            }
            Some(cookie)
        }
        None => None,
    };
    // SAFETY: `path` is a live NUL-terminated UTF-16 string; the DLL runs its
    // `DllMain` as it would from any loader.
    let module = unsafe {
        LoadLibraryExW(
            path.as_ptr(),
            core::ptr::null_mut(),
            LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_DEFAULT_DIRS,
        )
    };
    let loaded = if module.is_null() {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(module)
    };
    if let Some(cookie) = added {
        // SAFETY: `cookie` came from `AddDllDirectory` above and is removed once.
        unsafe { RemoveDllDirectory(cookie) };
    }
    loaded
}

/// The system folder, `System32` for the 64-bit game.
pub(crate) fn system_dir() -> Option<PathBuf> {
    let mut buffer = [0u16; 512];
    // SAFETY: the call writes at most the 512 units of the local `buffer`.
    let len = unsafe { GetSystemDirectoryW(buffer.as_mut_ptr(), 512) };
    let len = usize::try_from(len)
        .ok()
        .filter(|&len| len != 0 && len < buffer.len())?;
    Some(PathBuf::from(OsString::from_wide(&buffer[..len])))
}

/// An export as `GetProcAddress` returns it, to be transmuted to the
/// signature its module documents.
pub(crate) type Export = unsafe extern "system" fn() -> isize;

/// A module loaded in the process for as long as the loader uses it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Module(HMODULE);

impl Module {
    /// The module `name` when the process has loaded it, without loading it,
    /// pinned so it stays loaded for the process.
    pub(crate) fn loaded(name: &str) -> Option<Self> {
        let wide = wide(name);
        let mut module = core::ptr::null_mut();
        // SAFETY: `wide` is a live NUL-terminated UTF-16 string and `module` a
        // local the call writes; the pin keeps a found module mapped until the
        // process ends.
        let found = unsafe {
            GetModuleHandleExW(
                GET_MODULE_HANDLE_EX_FLAG_PIN,
                wide.as_ptr(),
                &raw mut module,
            )
        };
        (found != 0 && !module.is_null()).then_some(Self(module))
    }

    /// The system's own DLL `name`, loaded from the system folder by full
    /// path, so a DLL of that name in the game's folder is never taken for
    /// it; kept for the process.
    pub(crate) fn system(name: &str) -> Option<Self> {
        load_library(&system_dir()?.join(name), None).ok().map(Self)
    }

    /// The module behind `handle`.
    ///
    /// # Safety
    ///
    /// - `handle` is non-null: a null handle would make lookups search the
    ///   executable instead.
    /// - `handle` is a module loaded in this process that stays loaded while
    ///   the result or an export found through it is used.
    pub(crate) const unsafe fn from_handle(handle: HMODULE) -> Self {
        Self(handle)
    }

    /// The export `name`, or `None` when the module has none of that name.
    pub(crate) fn export(self, name: &str) -> Option<Export> {
        let mut name = name.as_bytes().to_vec();
        if name.contains(&0) {
            return None;
        }
        name.push(0);
        // SAFETY: `self.0` is a loaded module that stays loaded while used:
        // `loaded` pins it, `system` never releases its reference, and
        // `from_handle`'s caller vouches for it; `name` is a live
        // NUL-terminated string.
        unsafe { GetProcAddress(self.0, name.as_ptr()) }
    }
}

/// `text` as a NUL-terminated UTF-16 string.
fn wide(text: impl AsRef<OsStr>) -> Vec<u16> {
    text.as_ref().encode_wide().chain([0]).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_loaded_module_finds_its_export_and_not_a_missing_one() {
        let kernel = Module::loaded("kernel32.dll").expect("every process has kernel32");
        assert!(kernel.export("GetCurrentProcessId").is_some());
        assert!(kernel.export("NoSuchExportOfKernel32").is_none());
    }

    #[test]
    fn an_export_name_with_a_nul_inside_is_refused() {
        let kernel = Module::loaded("kernel32.dll").expect("every process has kernel32");
        assert!(kernel.export("Get\0CurrentProcessId").is_none());
    }

    #[test]
    fn a_system_library_loads_from_the_system_folder() {
        let dbghelp = Module::system("dbghelp.dll").expect("dbghelp loads");
        assert!(dbghelp.export("MiniDumpWriteDump").is_some());
        let path = system_dir().unwrap().join("dbghelp.dll");
        assert_eq!(Module::loaded(path.to_str().unwrap()), Some(dbghelp));
    }

    #[test]
    fn a_module_the_process_lacks_is_not_loaded() {
        assert!(Module::loaded("no-such-module-of-enshwick.dll").is_none());
    }
}
