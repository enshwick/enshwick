//! A mod of side server's controls: on a client they fire only while it
//! hosts, since in another's world the server carries them.

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use enshwick::abi::{Side, Status};
    use enshwick::actions::{Action, Outcome};
    use enshwick::controls::{Control, Key};
    use enshwick::i18n::Texts;
    use enshwick::{Load, Mod, Off};

    struct Weather;

    impl Mod for Weather {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let actions = load.actions(Texts::default())?;
            actions.add(Action::new("day", "Make it day"), |_| {
                Outcome::done("Morning")
            })?;
            let day = Control::new("day", "Make it day")
                .key(Key::parse("F8")?)
                .runs("day");
            load.control(day, |_| {})?;
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Weather,
        r#"
        id = "you.weather"
        side = "server"
        "#
    );

    /// How many calls F8 brings and how many runs it sends, on a client.
    fn press(hosting: bool) -> (usize, usize) {
        let loader = enshwick::test::FakeHost::new()
            .side(Side::CLIENT)
            .hosting(hosting)
            .install();
        // SAFETY: the exports of this file's `declare_mod!(Weather)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        let fired = loader.keys(&[Key::parse("F8").unwrap()], Duration::from_millis(16));
        (fired, loader.action_runs().len())
    }

    #[test]
    fn a_server_mod_s_control_does_not_fire_on_a_client_in_another_s_world() {
        assert_eq!(press(false), (0, 0));
    }

    #[test]
    fn a_server_mod_s_control_fires_and_runs_its_action_on_a_client_that_hosts() {
        assert_eq!(press(true), (1, 1));
    }
}
