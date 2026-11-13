//! A panic in a mod's ready, in a state dropped unused, in a read of the
//! manifest or in a data pack's init turns off what it should and leaves the
//! rest running.

#[cfg(test)]
mod tests {
    use enshwick::abi::{Level, ModHandle, Side, Status};
    use enshwick::test::FakeHost;
    use enshwick::{Host, Load, Mod, Off};

    struct Flaky;

    impl Mod for Flaky {
        fn init(_load: &mut Load<'_>) -> Result<Self, Off> {
            Ok(Self)
        }

        fn ready(&self, _host: Host) {
            panic!("the dawn never came")
        }
    }

    impl Drop for Flaky {
        fn drop(&mut self) {
            panic!("the lamp would not go out")
        }
    }

    enshwick::test::declare_mod!(
        Flaky,
        r#"
        id = "you.flaky"
        side = "server"
        "#
    );

    #[test]
    fn a_panic_in_ready_faults_the_mod_and_keeps_its_state() {
        let loader = FakeHost::new().install();
        // SAFETY: the exports of `declare_mod!` above.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        assert_eq!(loader.faults(), ["panic in ready: the dawn never came"]);
        assert!(Flaky::instance().is_some());
    }

    #[test]
    fn a_second_init_whose_state_panics_on_drop_is_refused_and_keeps_the_first() {
        let loader = FakeHost::new().install();
        // SAFETY: `enshwick_mod_init` is the export of `declare_mod!` above.
        let first = unsafe { loader.init(enshwick_mod_init) };
        // SAFETY: `enshwick_mod_init` is the export of `declare_mod!` above,
        // run a second time on the same fake table.
        let second = unsafe { loader.init(enshwick_mod_init) };
        assert_eq!((first, second), (Status::OK, Status::REFUSED));
        assert!(loader.faults().is_empty(), "{:?}", loader.faults());
        assert!(Flaky::instance().is_some());
    }

    #[test]
    fn a_panic_reading_the_manifest_leaves_it_readable() {
        let _loader = FakeHost::new().install();
        assert!(
            __ENSHWICK_MANIFEST
                .with_built(|_| panic!("a broken read"))
                .is_none()
        );
        assert!(!enshwick_mod_query().is_null());
    }

    #[test]
    fn a_panicking_pack_init_faults_the_pack_and_not_the_loader() {
        let loader = FakeHost::new().install();
        // SAFETY: the fake host's table lives for the process.
        unsafe { enshwick_base::__internal::install_host(loader.api(), ModHandle(9)) };
        // SAFETY: the fake host's table lives for the process.
        let status = unsafe {
            enshwick::__private::init_pack(loader.api(), ModHandle(1), Side::SERVER, |_| {
                panic!("the pack's rewards")
            })
        };
        assert_eq!(status, Status::FAULTED);
        let off: Vec<_> = loader
            .sent()
            .into_iter()
            .filter(|(_, level, line)| *level == Level::ERROR && line.starts_with("off:"))
            .collect();
        assert_eq!(
            off,
            [(
                ModHandle(1),
                Level::ERROR,
                "off: init panicked: the pack's rewards".to_owned()
            )]
        );
    }
}
