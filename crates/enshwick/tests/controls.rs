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
            load.control(
                Control::new("brake", "Brake")
                    .link(GameAction::Block)
                    .fires(&[Phase::Press, Phase::Hold, Phase::Release]),
                note,
            )?;
            load.control(Control::new("lights", "Lights").key(Key::parse("B")?), note)?;
            load.control(Control::new("dim", "Dim").key(Key::parse("Ctrl+B")?), note)?;
            load.control(
                Control::new("map", "Map")
                    .key(Key::parse("Tab")?)
                    .second(Key::parse("Shift+N")?)
                    .fires(&[Phase::Press, Phase::Release]),
                note,
            )?;
            load.control(Control::new("nitro", "Nitro").key(Key::parse("N")?), note)?;
            load.control(
                Control::new("flash", "Flash the lights")
                    .key(Key::parse("F")?)
                    .fires(&[Phase::Press, Phase::Release])
                    .runs("honk"),
                note,
            )?;
            assert!(
                load.control(Control::new("wipe", "Wipe").runs("Wipers"), note)
                    .is_err(),
                "an action id is 1 to 32 of [a-z0-9-]"
            );
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

    fn client() -> enshwick::test::Installed {
        start(enshwick::test::FakeHost::new().side(enshwick::abi::Side::CLIENT))
    }

    fn ann() -> PlayerInfo {
        PlayerInfo {
            header: Header::of::<PlayerInfo>(),
            slot: 0,
            entity: enshwick::Entity(2),
            key: enshwick::PlayerKey::new(7),
            permissions: 0,
            name_len: 0,
            name: [0; 64],
        }
    }

    #[test]
    fn a_key_fires_its_control_and_the_control_asks_the_server() {
        let loader = client();
        let listed: Vec<String> = loader.controls().into_iter().map(|c| c.0).collect();
        assert_eq!(
            listed,
            [
                "horn", "boost", "enter", "brake", "lights", "dim", "map", "nitro", "flash"
            ]
        );
        let tick = Duration::from_millis(16);
        assert_eq!(loader.keys(&[key("H")], tick), 1);
        assert_eq!(loader.keys(&[key("H")], tick), 0, "held, not repeated");
        assert_eq!(loader.keys(&[], tick), 0, "no release asked for");
        assert_eq!(taken(), [("horn".to_owned(), Phase::Press, None)]);
        assert_eq!(loader.action_runs(), [("honk".to_owned(), "b1".to_owned())]);
    }

    #[test]
    fn a_control_that_runs_an_action_asks_the_server_each_time_it_fires() {
        let loader = client();
        let tick = Duration::from_millis(16);
        assert_eq!(loader.keys(&[key("F")], tick), 1);
        assert_eq!(loader.keys(&[key("F")], tick), 0, "held, not repeated");
        assert_eq!(loader.keys(&[], tick), 1);
        assert_eq!(
            taken(),
            [
                ("flash".to_owned(), Phase::Press, None),
                ("flash".to_owned(), Phase::Release, None)
            ]
        );
        let honk = ("honk".to_owned(), String::new());
        assert_eq!(loader.action_runs(), [honk.clone(), honk]);
        assert_eq!(loader.action_targets(), [None, None]);
    }

    #[test]
    fn a_server_reads_no_keys() {
        let loader = start(enshwick::test::FakeHost::new().side(enshwick::abi::Side::SERVER));
        assert_eq!(loader.keys(&[key("H")], Duration::from_millis(16)), 0);
        assert!(taken().is_empty());
    }

    #[test]
    fn a_client_in_another_s_world_follows_no_game_action() {
        let interact = [GameAction::ContextualAction];
        let step = Duration::from_millis(50);
        assert_eq!(client().game_actions(&ann(), &interact, step), 0);
        assert!(taken().is_empty());
    }

    #[test]
    fn a_plain_key_fires_while_the_player_sprints() {
        let loader = client();
        assert_eq!(loader.keys(&[key("Shift+H")], Duration::from_millis(16)), 1);
        assert_eq!(taken(), [("horn".to_owned(), Phase::Press, None)]);
    }

    #[test]
    fn the_control_with_the_most_held_modifiers_wins_its_key() {
        let loader = client();
        let tick = Duration::from_millis(16);
        let fired = |held: &[Key]| {
            assert_eq!(loader.keys(held, tick), 1);
            let _up = loader.keys(&[], tick);
            taken().remove(0).0
        };
        assert_eq!(fired(&[key("Shift+B")]), "boost");
        assert_eq!(fired(&[key("B")]), "lights");
        assert_eq!(fired(&[key("Ctrl+B")]), "dim");
        assert_eq!(fired(&[key("Alt+B")]), "lights");
        assert_eq!(
            loader.keys(&[key("Ctrl+Shift+B")], tick),
            2,
            "a tie fires both"
        );
        let names: Vec<String> = taken().into_iter().map(|f| f.0).collect();
        assert_eq!(names, ["boost", "dim"]);
    }

    #[test]
    fn a_modifier_change_while_the_key_is_held_fires_no_release_and_no_press() {
        let loader = client();
        let tick = Duration::from_millis(16);
        let looks: Vec<usize> = [&[key("Shift+B")][..], &[key("B")], &[key("Ctrl+B")], &[]]
            .into_iter()
            .map(|held| loader.keys(held, tick))
            .collect();
        assert_eq!(looks, [1, 0, 0, 1]);
        assert_eq!(
            taken(),
            [
                ("boost".to_owned(), Phase::Press, None),
                ("boost".to_owned(), Phase::Release, None)
            ]
        );
    }

    #[test]
    fn a_control_stays_down_only_through_the_key_that_brought_it_down() {
        let loader = client();
        let tick = Duration::from_millis(16);
        let looks: Vec<usize> = [&[key("Tab")][..], &[key("Tab"), key("N")], &[key("N")]]
            .into_iter()
            .map(|held| loader.keys(held, tick))
            .collect();
        assert_eq!(looks, [1, 1, 1]);
        assert_eq!(
            taken(),
            [
                ("map".to_owned(), Phase::Press, None),
                ("nitro".to_owned(), Phase::Press, None),
                ("map".to_owned(), Phase::Release, None)
            ]
        );
    }

    #[test]
    fn a_system_combination_fires_no_control_on_its_key() {
        let loader = client();
        let tick = Duration::from_millis(16);
        assert_eq!(loader.keys(&[key("Tab"), key("Alt+G")], tick), 0, "Alt+Tab");
        assert!(taken().is_empty());
    }

    #[test]
    fn a_modifier_held_counts_for_every_key_held() {
        let loader = client();
        let tick = Duration::from_millis(16);
        assert_eq!(loader.keys(&[key("Shift+G"), key("N")], tick), 1);
        assert_eq!(taken(), [("map".to_owned(), Phase::Press, None)]);
    }

    #[test]
    fn a_chord_holds_once_past_its_threshold() {
        let loader = client();
        let tick = Duration::from_millis(100);
        let looks: Vec<usize> = [true, true, true, true, true, false]
            .into_iter()
            .map(|down| {
                let held: &[Key] = if down { &[key("Shift+B")] } else { &[] };
                loader.keys(held, tick)
            })
            .collect();
        assert_eq!(looks, [1, 0, 0, 1, 0, 1]);
        let phases: Vec<Phase> = taken().into_iter().map(|f| f.1).collect();
        assert_eq!(phases, [Phase::Press, Phase::Hold, Phase::Release]);
    }

    #[test]
    fn a_linked_control_fires_on_the_server_for_the_player_whose_action_fired() {
        let loader = start(enshwick::test::FakeHost::new());
        let player = ann();
        let step = Duration::from_millis(50);
        assert_eq!(loader.game_actions(&player, &[GameAction::Jump], step), 0);
        let interact = [GameAction::ContextualAction];
        assert_eq!(loader.game_actions(&player, &interact, step), 1);
        assert_eq!(loader.game_actions(&player, &interact, step), 0, "held");
        assert_eq!(taken(), [("enter".to_owned(), Phase::Press, Some(7))]);
    }

    #[test]
    fn a_held_game_action_holds_and_releases_its_linked_control() {
        let loader = start(enshwick::test::FakeHost::new());
        let player = ann();
        let block = [GameAction::Block];
        let step = Duration::from_millis(300);
        let looks: Vec<usize> = [&block[..], &block, &block, &[]]
            .into_iter()
            .map(|held| loader.game_actions(&player, held, step))
            .collect();
        assert_eq!(looks, [1, 0, 1, 1]);
        let phases: Vec<Phase> = taken().into_iter().map(|f| f.1).collect();
        assert_eq!(phases, [Phase::Press, Phase::Hold, Phase::Release]);
    }

    #[test]
    fn the_player_s_binding_replaces_the_mod_s_default_slot_by_slot() {
        let loader = start(
            enshwick::test::FakeHost::new()
                .side(enshwick::abi::Side::CLIENT)
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
        assert_eq!(loader.keys(&[key("H")], tick), 0);
        assert_eq!(loader.keys(&[key("J")], tick), 1);
    }
}
