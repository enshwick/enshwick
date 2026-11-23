//! A client mod reads the keys the player bound to the game's controls and
//! rebinds one, and the loader refuses a server, an action the build lacks
//! and a read before the interface's first frame.

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use enshwick::abi::{Side, Status};
    use enshwick::controls::Key;
    use enshwick::input::{Binding, BindingError, BoundKey, Input};
    use enshwick::typed::input_mapping::runtime::GameplayAction;
    use enshwick::{Load, Mod, Off};

    static INPUT: OnceLock<Input> = OnceLock::new();

    struct Keys;

    impl Mod for Keys {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let _first = INPUT.set(load.input()?);
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Keys,
        r#"
        id = "you.keys"
        side = "both"
        "#
    );

    fn key(name: &str) -> Key {
        Key::parse(name).unwrap()
    }

    fn input(side: Side) -> (enshwick::test::Installed, Input) {
        let loader = enshwick::test::FakeHost::new()
            .player_input()
            .side(side)
            .install();
        // SAFETY: the three functions are the exports of this file's `declare_mod!(Keys)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        (loader, *INPUT.get().unwrap())
    }

    #[test]
    fn a_binding_reads_once_the_interface_drew_and_names_an_action_the_build_lacks() {
        let (loader, input) = input(Side::CLIENT);
        loader.bind_game_action("ContextualAction", Some(key("E")), Some(key("Mouse4")));
        loader.bind_game_action("Jump", Some(key("Space")), None);
        assert_eq!(
            input.binding(&GameplayAction::ContextualAction),
            Err(BindingError::Unavailable),
            "no frame read the bindings yet"
        );
        let _drawn = loader.draw_frame();
        let interact = input.binding(&GameplayAction::ContextualAction).unwrap();
        assert_eq!(
            (interact.first, interact.second),
            (
                Some(BoundKey::Key(key("E"))),
                Some(BoundKey::Key(key("Mouse4")))
            )
        );
        assert_eq!(
            input.binding(&GameplayAction::Sneak),
            Err(BindingError::NoAction("Sneak".to_owned()))
        );
    }

    #[test]
    fn a_rebind_shows_after_the_next_frame_and_gives_the_player_s_key_back() {
        let (loader, input) = input(Side::CLIENT);
        loader.bind_game_action("Jump", Some(key("Space")), Some(key("Mouse5")));
        let _drawn = loader.draw_frame();
        input.rebind(&GameplayAction::Jump, key("J")).unwrap();
        assert_eq!(loader.game_rebind("Jump"), Some(key("J")));
        let first = |input: Input| {
            input
                .binding(&GameplayAction::Jump)
                .map(|b: Binding| b.first)
        };
        assert_eq!(
            first(input),
            Ok(Some(BoundKey::Key(key("Space")))),
            "until a frame"
        );
        let _drawn = loader.draw_frame();
        let jump = input.binding(&GameplayAction::Jump).unwrap();
        assert_eq!(jump.first, Some(BoundKey::Key(key("J"))));
        assert_eq!(
            jump.second,
            Some(BoundKey::Key(key("Mouse5"))),
            "the second key stays"
        );
        input.reset_binding(&GameplayAction::Jump).unwrap();
        assert_eq!(loader.game_rebind("Jump"), None);
        let _drawn = loader.draw_frame();
        assert_eq!(first(input), Ok(Some(BoundKey::Key(key("Space")))));
        assert_eq!(
            input.rebind(&GameplayAction::Sneak, key("J")),
            Err(BindingError::NoAction("Sneak".to_owned()))
        );
    }

    #[test]
    fn a_server_has_no_bindings_to_read_or_rebind() {
        let (loader, input) = input(Side::SERVER);
        loader.bind_game_action("Jump", Some(key("Space")), None);
        assert_eq!(
            input.binding(&GameplayAction::Jump),
            Err(BindingError::Unavailable)
        );
        assert_eq!(
            input.rebind(&GameplayAction::Jump, key("J")),
            Err(BindingError::Unavailable)
        );
        assert_eq!(loader.game_rebind("Jump"), None);
    }
}
