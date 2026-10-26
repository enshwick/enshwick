//! Modules of the process and their exports, found by name.

use windows_sys::Win32::Foundation::HMODULE;
use windows_sys::Win32::System::LibraryLoader::{
    GET_MODULE_HANDLE_EX_FLAG_PIN, GetModuleHandleExW, GetProcAddress, LoadLibraryW,
};

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

    /// The module `name`, loaded now if the process has not yet, kept for
    /// the process.
    pub(crate) fn load(name: &str) -> Option<Self> {
        let wide = wide(name);
        // SAFETY: `wide` is a live NUL-terminated UTF-16 string; the
        // reference the call takes is never released, so the module stays.
        let module = unsafe { LoadLibraryW(wide.as_ptr()) };
        (!module.is_null()).then_some(Self(module))
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
        // `loaded` pins it, `load` never releases its reference, and
        // `from_handle`'s caller vouches for it; `name` is a live
        // NUL-terminated string.
        unsafe { GetProcAddress(self.0, name.as_ptr()) }
    }
}

/// `text` as a NUL-terminated UTF-16 string.
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain([0]).collect()
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
    fn a_loaded_library_finds_its_export() {
        let kernel = Module::load("kernel32.dll").expect("kernel32 loads");
        assert!(kernel.export("GetCurrentProcessId").is_some());
    }

    #[test]
    fn a_module_the_process_lacks_is_not_loaded() {
        assert!(Module::loaded("no-such-module-of-enshwick.dll").is_none());
    }
}
