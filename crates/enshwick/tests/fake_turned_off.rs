//! The fake host's refusals a test sets reach the mod as the loader's would:
//! a refused hook and a build the loader found incomplete turn off a mod
//! whose init needs them, and a buff the mod gave is read back.

/// A mod whose init hooks one target and needs every entry of the build.
#[cfg(test)]
mod needs_hook_and_build {
    use enshwick::abi::Status;
    use enshwick::test::{FakeHost, address};
    use enshwick::{Load, Mod, Next, Off};

    type Count = extern "system" fn(u32) -> u32;

    extern "system" fn counted(n: u32) -> u32 {
        n
    }

    fn count(next: Next<'_, Count>, n: u32) -> u32 {
        next.call(n) + 1
    }

    enshwick::detour! {
        static COUNT: extern "system" fn(n: u32) -> u32 = count;
    }

    struct Needs;

    impl Mod for Needs {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            if load.host().build().complete == 0 {
                return Err(Off::new("the build lacks entries this mod needs"));
            }
            let target = load.symbol("count")?;
            // SAFETY: `count` takes and returns one number, as `COUNT` does.
            unsafe { load.hook(&COUNT, target) }?;
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Needs,
        r#"
        id = "you.needs"
        side = "server"
        "#
    );

    fn host() -> FakeHost {
        FakeHost::new().symbol("count", address(counted as Count))
    }

    #[test]
    fn a_mod_whose_hook_the_host_refuses_is_off_and_its_hook_never_links() {
        let loader = host().refuse_hook(address(counted as Count)).install();
        // SAFETY: the export of this module's `declare_mod!`.
        let status = unsafe { loader.init(enshwick_mod_init) };
        assert_ne!(status, Status::OK);
        assert!(loader.is_off());
        assert!(
            loader.detour(&COUNT).is_none(),
            "the refused hook is not linked"
        );
    }

    #[test]
    fn a_mod_that_needs_a_complete_build_is_off_on_an_incomplete_one() {
        let loader = host().build_complete(false).install();
        // SAFETY: the export of this module's `declare_mod!`.
        let status = unsafe { loader.init(enshwick_mod_init) };
        assert_ne!(status, Status::OK);
        assert!(loader.is_off());
    }

    #[test]
    fn the_same_mod_on_a_complete_build_with_its_hook_free_starts_and_links() {
        let loader = host().install();
        // SAFETY: the export of this module's `declare_mod!`.
        let status = unsafe { loader.init(enshwick_mod_init) };
        assert_eq!(status, Status::OK);
        assert!(!loader.is_off());
        let linked = loader.detour(&COUNT).expect("linked");
        assert_eq!(linked(4), 5);
    }
}
