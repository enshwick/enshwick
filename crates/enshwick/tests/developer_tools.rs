//! A mod with `developer: true` runs only while developer tools are on, and
//! the fake host refuses it the same way the loader does.

#[cfg(test)]
mod tests {
    use enshwick::abi::{Side, Status};
    use enshwick::test::FakeHost;

    struct Inspector;

    impl enshwick::Mod for Inspector {
        fn init(_load: &mut enshwick::Load<'_>) -> Result<Self, enshwick::Off> {
            enshwick::info!("inspector on");
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Inspector,
        r#"
        id = "you.inspector"
        side = "both"
        developer = true
        "#
    );

    fn start(host: FakeHost) -> (Status, enshwick::test::Installed) {
        let loader = host.install();
        // SAFETY: the exports of the `declare_mod!` above.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        (status, loader)
    }

    fn ran(loader: &enshwick::test::Installed) -> bool {
        loader.logs().iter().any(|(_, line)| line == "inspector on")
    }

    #[test]
    fn without_developer_tools_a_developer_mod_is_off_before_init() {
        let hosted = FakeHost::new().side(Side::CLIENT).hosting(true);
        for host in [FakeHost::new(), FakeHost::new().side(Side::CLIENT), hosted] {
            let (status, loader) = start(host);
            assert_eq!(status, Status::REFUSED);
            assert!(!ran(&loader));
            assert_eq!(loader.faults(), [enshwick::mods::DEVELOPER_TOOL_OFF]);
            assert!(
                loader
                    .logs()
                    .iter()
                    .any(|(level, line)| *level == enshwick::abi::Level::ERROR
                        && line
                            == &format!(
                                "off: {} ({})",
                                enshwick::mods::DEVELOPER_TOOL_OFF,
                                enshwick::__private::MOD_OFF
                            )),
                "{:?}",
                loader.logs()
            );
        }
    }

    #[test]
    fn the_fake_never_calls_the_init_export_of_a_refused_mod() {
        use std::sync::atomic::{AtomicU32, Ordering};
        static CALLS: AtomicU32 = AtomicU32::new(0);
        // An init of its own, as a mod written in C exports one.
        unsafe extern "system" fn counted(
            _: *const enshwick::abi::HostApi,
            _: enshwick::abi::ModHandle,
        ) -> Status {
            CALLS.fetch_add(1, Ordering::Relaxed);
            Status::OK
        }
        let loader = FakeHost::new().install();
        // SAFETY: the query and ready exports of the `declare_mod!` above,
        // with an init that touches nothing.
        let status = unsafe { loader.start(enshwick_mod_query, counted, enshwick_mod_ready) };
        assert_eq!(status, Status::REFUSED);
        assert_eq!(CALLS.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn a_developer_mod_runs_with_developer_tools_on() {
        let (status, loader) = start(FakeHost::new().developer_tools(true));
        assert_eq!(status, Status::OK);
        assert!(ran(&loader));
        assert!(loader.faults().is_empty());
    }
}
