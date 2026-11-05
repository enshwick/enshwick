//! `enshwick.controls`: a script declares controls as tables, a key fires
//! them on the client and a game action on the server, and a control asks
//! the server to run an action, as a Rust mod's.

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use enshwick::abi::{Header, PlayerInfo, Side};
    use enshwick::controls::{GameAction, Key};
    use enshwick::test::FakeHost;

    use crate::harness::{logged, run};

    const MAIN: &str = r#"
        local actions = enshwick.actions.actions(enshwick.i18n.parse(""))
        actions:add({ id = "honk", label = "Honk" }, function()
            return enshwick.actions.done("Honk")
        end)
        enshwick.controls.control({ name = "horn", label = "Sound the horn", key = "H", second = "Mouse4" }, function(fired)
            enshwick.log.info(`fired {fired.control} {fired.phase} {fired.player == nil}`)
            actions:run("honk", { { Toggle = true } })
        end)
        enshwick.controls.control({
            name = "boost",
            label = "Boost",
            key = "shift + b",
            fires = { "Press", "Hold", "Release" },
            hold_for = 0.3,
        }, function(fired)
            enshwick.log.info(`fired {fired.control} {fired.phase}`)
        end)
        enshwick.controls.control({ name = "enter", label = "Enter the car", link = "ContextualAction" }, function(fired)
            enshwick.log.info(`fired {fired.control} {fired.phase} by {fired.player.key}`)
        end)
        local ok = pcall(enshwick.controls.control, { name = "horn", label = "Again" }, function() end)
        enshwick.log.info(`again {ok}`)
        local key = enshwick.controls.parse("ctrl + f5")
        enshwick.log.info(`key {key} mouse {enshwick.controls.is_mouse("Mouse4")} {enshwick.controls.is_mouse("E")} plain {enshwick.controls.modifiers("E")} {enshwick.controls.modifiers(key) > 1} same {enshwick.controls.code(key) == enshwick.controls.code("F5")} {enshwick.controls.code("E") == enshwick.controls.code("F5")}`)
        ok = pcall(enshwick.controls.parse, "Escape")
        enshwick.log.info(`escape {ok}`)
    "#;

    fn key(name: &str) -> Key {
        Key::parse(name).unwrap()
    }

    #[test]
    fn a_key_fires_a_script_s_control_and_the_control_asks_the_server() {
        let loader = run(FakeHost::new().side(Side::CLIENT), MAIN);
        for line in [
            "again false",
            "key Ctrl+F5 mouse true false plain 0 true same true false",
            "escape false",
        ] {
            assert!(logged(&loader, line), "{line}: {:?}", loader.logs());
        }
        let listed: Vec<String> = loader.controls().into_iter().map(|c| c.0).collect();
        assert_eq!(listed, ["horn", "boost", "enter"]);

        let tick = Duration::from_millis(16);
        assert_eq!(loader.keys(&[key("H")], tick), 1);
        assert_eq!(loader.keys(&[], tick), 0, "no release asked for");
        assert!(
            logged(&loader, "fired horn Press true"),
            "{:?}",
            loader.logs()
        );
        assert_eq!(loader.action_runs(), [("honk".to_owned(), "b1".to_owned())]);

        assert_eq!(loader.keys(&[key("Shift+B")], tick), 1);
        assert_eq!(
            loader.keys(&[key("Shift+B")], Duration::from_millis(400)),
            1
        );
        assert_eq!(loader.keys(&[], tick), 1);
        for phase in ["Press", "Hold", "Release"] {
            let line = format!("fired boost {phase}");
            assert!(logged(&loader, &line), "{line}: {:?}", loader.logs());
        }
    }

    #[test]
    fn a_game_action_fires_a_script_s_linked_control_with_its_player() {
        let loader = run(FakeHost::new(), MAIN);
        let ann = PlayerInfo {
            header: Header::of::<PlayerInfo>(),
            slot: 0,
            entity: enshwick::Entity(2),
            key: enshwick::PlayerKey::new(7),
            permissions: 0,
            name_len: 0,
            name: [0; 64],
        };
        let fired = loader.game_actions(
            &ann,
            &[GameAction::ContextualAction],
            Duration::from_millis(50),
        );
        assert_eq!(fired, 1, "{:?}", loader.logs());
        assert!(
            logged(&loader, "fired enter Press by 7"),
            "{:?}",
            loader.logs()
        );
    }
}
