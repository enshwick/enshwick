//! The C example builds into a DLL that reads as a mod and starts.

#![cfg(windows)]

#[cfg(test)]
mod tests {
    use std::os::windows::ffi::OsStrExt as _;

    use enshwick::abi::{ABI_MAJOR, Level, Manifest, Status, exports};
    use enshwick::test::{FakeHost, InitFn};
    use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};

    /// The address of the export `name` of the C example, which stays loaded.
    fn export(name: &str) -> unsafe extern "system" fn() -> isize {
        let path = std::path::Path::new(env!("HELLO_C_DLL"));
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
        // SAFETY: `wide` ends in the NUL chained onto it and lives across the call.
        let module = unsafe { LoadLibraryW(wide.as_ptr()) };
        assert!(!module.is_null(), "{} does not load", path.display());
        let name = format!("{name}\0");
        // SAFETY: `module` is loaded and never freed; `name` ends in a NUL.
        unsafe { GetProcAddress(module, name.as_ptr()) }.expect("the export exists")
    }

    #[test]
    fn the_c_mod_reads_as_a_mod_and_starts() {
        // SAFETY: the ABI gives `exports::ABI` the signature `fn() -> u32`.
        let abi: unsafe extern "system" fn() -> u32 =
            unsafe { core::mem::transmute(export(exports::ABI)) };
        // SAFETY: `abi` takes nothing and returns a constant.
        assert_eq!(unsafe { abi() }, u32::from(ABI_MAJOR));

        // SAFETY: the ABI gives `exports::QUERY` the signature
        // `fn() -> *const Manifest`.
        let query: unsafe extern "system" fn() -> *const Manifest =
            unsafe { core::mem::transmute(export(exports::QUERY)) };
        // SAFETY: `query` takes nothing and returns a pointer to its static
        // manifest.
        let manifest = unsafe { query() };
        assert!(!manifest.is_null());
        // SAFETY: `manifest` is non-null, checked just before, and points at a
        // static manifest no thread writes, in a DLL that is never unloaded.
        let manifest = unsafe { &*manifest };
        assert_eq!(manifest.header.size as usize, size_of::<Manifest>());
        // SAFETY: the manifest's strings are static literals of the DLL.
        assert_eq!(unsafe { manifest.id.as_str() }, Some("you.hello-c"));

        // SAFETY: the ABI gives `exports::INIT` the signature of `InitFn`.
        let init: InitFn = unsafe { core::mem::transmute(export(exports::INIT)) };
        let loader = FakeHost::new().install();
        // SAFETY: `init` is the C mod's `enshwick_mod_init`.
        assert_eq!(unsafe { loader.init(init) }, Status::OK);
        assert!(
            loader
                .logs()
                .contains(&(Level::INFO, "hello from C".to_owned())),
            "{:?}",
            loader.logs()
        );
    }
}
