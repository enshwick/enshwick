//! A mod built against the last published API, `tests/old-mod`, loads in
//! the current host: its ABI major matches, its manifest holds every field
//! of that major, and it starts against the current host table.

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::process::Command;

    use enshwick::abi::{ABI_MAJOR, Manifest, Status, read_headed};
    use enshwick::test::{FakeHost, InitFn, ReadyFn};

    /// Where the fields every manifest of this major has end, as the loader
    /// reads them.
    const REQUIRED: usize = size_of::<Manifest>();

    /// Builds the old mod and returns its library, or `None` where this
    /// process cannot run cargo, as under Wine.
    fn build_old_mod() -> Option<PathBuf> {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/old-mod");
        let target = Path::new(env!("CARGO_TARGET_TMPDIR")).join("old-mods");
        let status = Command::new(env!("CARGO"))
            .args(["build", "--locked", "--quiet", "--manifest-path"])
            .arg(fixture.join("Cargo.toml"))
            .arg("--target-dir")
            .arg(&target)
            .env_remove("RUSTFLAGS")
            .status()
            .ok()?;
        assert!(status.success(), "the old mod did not build");
        Some(
            target
                .join("debug")
                .join(libloading::library_filename("old_mod")),
        )
    }

    #[test]
    #[ignore = "the fixture pins a release of ABI 3; the first ABI 4 release repins it"]
    fn a_mod_built_against_the_last_release_starts_in_the_current_host() {
        let Some(path) = build_old_mod() else {
            return;
        };
        // SAFETY: the old mod's initialisers only set up its statics.
        let library = unsafe { libloading::Library::new(&path) }.unwrap();
        // SAFETY: `enshwick_mod_abi` has the signature `abi::exports::ABI`
        // documents, which the ABI keeps across minors.
        let abi =
            *unsafe { library.get::<extern "system" fn() -> u32>(b"enshwick_mod_abi") }.unwrap();
        // SAFETY: `enshwick_mod_query` has the signature `abi::exports::QUERY`
        // documents, which the ABI keeps across minors.
        let query = *unsafe {
            library.get::<extern "system" fn() -> *const Manifest>(b"enshwick_mod_query")
        }
        .unwrap();
        // SAFETY: `enshwick_mod_init` has the signature `abi::exports::INIT`
        // documents, which the ABI keeps across minors.
        let init = *unsafe { library.get::<InitFn>(b"enshwick_mod_init") }.unwrap();
        // SAFETY: `enshwick_mod_ready` has the signature `abi::exports::READY`
        // documents, which the ABI keeps across minors.
        let ready = *unsafe { library.get::<ReadyFn>(b"enshwick_mod_ready") }.unwrap();
        assert_eq!(abi(), u32::from(ABI_MAJOR));

        // SAFETY: the old mod's static manifest, readable for the size its
        // header states, lives as long as `library`.
        let manifest = unsafe { read_headed(query(), REQUIRED) }.unwrap();
        // SAFETY: a `'static` string of the old mod's manifest, alive while
        // `library` is loaded.
        assert_eq!(unsafe { manifest.id.as_str() }, Some("you.old-mod"));

        let host = FakeHost::new().install();
        // SAFETY: `init` and `ready` are the exports of the old mod's one
        // `declare_mod!`, started once, while `library` stays loaded.
        assert_eq!(unsafe { host.start(init, ready) }, Status::OK);
        drop(host);
        drop(library);
    }
}
