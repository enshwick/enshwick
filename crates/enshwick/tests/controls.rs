//! Controls: declared in init, fired by keys on the client and by game
//! actions on the server, rebound by the player, and a control's request to
//! the server.

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::time::Duration;

    use enshwick::abi::{Header, PlayerInfo, Status};
    use enshwick::actions::{Action, Outcome, Value};
    use enshwick::controls::{Control, GameAction, Key, Phase};
    use enshwick::i18n::Texts;
    use enshwick::{Load, Mod, Off};

    /// Every firing: control, phase and the player's key on the server.
    static FIRED: Mutex<Vec<(String, Phase, Option<u64>)>> = Mutex::new(Vec::new());

    struct Car;

    impl Mod for Car {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let actions = load.actions(Texts::default())?;
            actions.add(Action::new("honk", "Honk"), |_| Outcome::done("Honk"))?;
            let note = |fired: &enshwick::controls::Fired<'_>| {
                FIRED.lock().unwrap().push((
                    fired.control.to_owned(),
                    fired.phase,
                    fired.player.as_ref().map(|p| p.key.get()),
                ));
            };
            load.control(
                Control::new("horn", "Sound the horn")
                    .key(Key::parse("H")?)
                    .second(Key::parse("Mouse4")?),
                move |fired| {
                    note(fired);
                    let _sent = actions.run("honk", &[Value::Toggle(true)]);
                },
            )?;
            load.control(
                Control::new("boost", "Boost")
                    .key(Key::parse("Shift+B")?)
                    .fires(&[Phase::Press, Phase::Hold, Phase::Release])
                    .hold_for(Duration::from_millis(300)),
                note,
            )?;
            load.control(
                Control::new("enter", "Enter the car").link(GameAction::ContextualAction),
                note,
            )?;
            assert!(
                load.control(Control::new("horn", "Again"), note).is_err(),
                "a name is declared once"
            );
            assert!(load.control(Control::new("Bad Name", "x"), note).is_err());
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Car,
        r#"
        id = "you.cars"
        side = "both"
        "#
    );

    fn taken() -> Vec<(String, Phase, Option<u64>)> {
        std::mem::take(&mut *FIRED.lock().unwrap())
    }

    fn key(name: &str) -> Key {
        Key::parse(name).unwrap()
    }

    fn start(host: enshwick::test::FakeHost) -> enshwick::test::Installed {
        let loader = host.install();
        // SAFETY: the exports of this file's `declare_mod!(Car)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        loader
    }

    #[test]
    fn a_key_fires_its_control_and_the_control_asks_the_server() {
        let loader = start(enshwick::test::FakeHost::new().side(enshwick::abi::Side::CLIENT));
        let listed: Vec<String> = loader.controls().into_iter().map(|c| c.0).collect();
        assert_eq!(listed, ["horn", "boost", "enter"]);
        let tick = Duration::from_millis(16);
        assert_eq!(loader.keys(&[key("H")], &[], tick), 1);
        assert_eq!(loader.keys(&[key("H")], &[], tick), 0, "held, not repeated");
        assert_eq!(loader.keys(&[], &[], tick), 0, "no release asked for");
        assert_eq!(taken(), [("horn".to_owned(), Phase::Press, None)]);
        assert_eq!(loader.action_runs(), [("honk".to_owned(), "b1".to_owned())]);
        assert_eq!(
            loader.keys(&[], &[key("H")], tick),
            1,
            "a tap between looks"
        );
    }

    #[test]
    fn a_chord_holds_once_past_its_threshold_and_needs_exactly_its_modifiers() {
        let loader = start(enshwick::test::FakeHost::new());
        let tick = Duration::from_millis(100);
        assert_eq!(loader.keys(&[key("B")], &[], tick), 0, "Shift is missing");
        assert_eq!(loader.keys(&[key("Ctrl+Shift+B")], &[], tick), 0);
        let looks: Vec<usize> = [true, true, true, true, true, false]
            .into_iter()
            .map(|down| {
                let held: &[Key] = if down { &[key("Shift+B")] } else { &[] };
                loader.keys(held, &[], tick)
            })
            .collect();
        assert_eq!(looks, [1, 0, 0, 1, 0, 1]);
        let phases: Vec<Phase> = taken().into_iter().map(|f| f.1).collect();
        assert_eq!(phases, [Phase::Press, Phase::Hold, Phase::Release]);
    }

    #[test]
    fn a_linked_control_fires_on_the_server_for_the_player_whose_action_fired() {
        let loader = start(enshwick::test::FakeHost::new());
        let player = PlayerInfo {
            header: Header::of::<PlayerInfo>(),
            slot: 0,
            entity: enshwick::Entity(2),
            key: enshwick::PlayerKey::new(7),
            permissions: 0,
            name_len: 0,
            name: [0; 64],
        };
        assert_eq!(loader.game_action(&player, GameAction::Jump), 0);
        assert_eq!(loader.game_action(&player, GameAction::ContextualAction), 1);
        assert_eq!(taken(), [("enter".to_owned(), Phase::Press, Some(7))]);
    }

    #[test]
    fn the_player_s_binding_replaces_the_mod_s_default_slot_by_slot() {
        let loader = start(
            enshwick::test::FakeHost::new()
                .bind("horn", 0, Some(key("J")))
                .bind("boost", 0, None),
        );
        let keys = |name: &str| {
            loader
                .controls()
                .into_iter()
                .find(|c| c.0 == name)
                .unwrap()
                .2
        };
        assert_eq!(keys("horn"), [Some(key("J")), Some(key("Mouse4"))]);
        assert_eq!(keys("boost"), [None, None]);
        let tick = Duration::from_millis(16);
        assert_eq!(loader.keys(&[key("H")], &[], tick), 0);
        assert_eq!(loader.keys(&[key("J")], &[], tick), 1);
    }
}
