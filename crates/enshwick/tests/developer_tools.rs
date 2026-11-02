//! A dedicated server runs a mod with `developer: true` only while developer
//! tools are on, and the fake host refuses it the same way.

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
    fn a_server_without_developer_tools_refuses_a_developer_mod_before_init() {
        let (status, loader) = start(FakeHost::new().developer_tools(false));
        assert_eq!(status, Status::REFUSED);
        assert!(!ran(&loader));
        assert!(
            loader
                .logs()
                .iter()
                .any(|(_, line)| line.contains("mod-data/enshwick/developer-tools")),
            "{:?}",
            loader.logs()
        );
    }

    #[test]
    fn a_developer_mod_runs_with_developer_tools_on() {
        let (status, loader) = start(FakeHost::new());
        assert_eq!(status, Status::OK);
        assert!(ran(&loader));
    }

    #[test]
    fn a_client_runs_a_developer_mod_without_developer_tools() {
        let (status, loader) = start(FakeHost::new().side(Side::CLIENT).developer_tools(false));
        assert_eq!(status, Status::OK);
        assert!(ran(&loader));
    }
}
