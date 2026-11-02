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
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
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
    fn a_developer_mod_runs_with_developer_tools_on() {
        let (status, loader) = start(FakeHost::new().developer_tools(true));
        assert_eq!(status, Status::OK);
        assert!(ran(&loader));
        assert!(loader.faults().is_empty());
    }
}
