//! The client snapshot through the safe layer on the fake host.
#[cfg(test)]
mod tests {
    use enshwick::abi::{ClientSnapshot as Raw, Side, Status, Str};
    use enshwick::controls::GameAction;
    use enshwick::input::{Input, SnapshotPart, SnapshotState};
    use enshwick::{Entity, Load, Mod, Off, RecipeId, WorldError};
    use std::sync::Mutex;

    static INPUT: Mutex<Option<Input>> = Mutex::new(None);

    struct Watcher;

    impl Mod for Watcher {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            *INPUT.lock().unwrap() = Some(load.input()?);
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Watcher,
        r#"id = "you.client-snapshot"
side = "both""#
    );

    fn input(side: Side) -> (enshwick::test::Installed, Input) {
        let loader = enshwick::test::FakeHost::new()
            .side(side)
            .player_input()
            .install();
        // SAFETY: these are this file's generated exports.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let input = INPUT.lock().unwrap().unwrap();
        (loader, input)
    }

    #[test]
    fn a_snapshot_before_the_first_frame_is_never_read_and_names_nothing() {
        let (_loader, input) = input(Side::CLIENT);
        let snapshot = input.client_snapshot().unwrap();
        assert_eq!(snapshot.state(), SnapshotState::Never);
        assert_eq!(snapshot.session(), 0);
        assert_eq!(snapshot.age_ms(), None);
        assert_eq!(snapshot.hud_screen(), None);
        assert!(snapshot.held().is_none());
    }

    #[test]
    fn a_snapshot_names_the_tab_apart_from_the_screen_and_maps_held_bits() {
        let (loader, input) = input(Side::CLIENT);
        let block = GameAction::ALL
            .iter()
            .position(|action| *action == GameAction::Block)
            .unwrap();
        loader.set_client_snapshot(Raw {
            state: Raw::FRESH,
            available: Raw::SCREENS | Raw::MENU_TAB | Raw::HELD | Raw::PLAYER | Raw::RECIPE,
            session: 3,
            age_ms: 16,
            scene: Str::new("world1"),
            hud_screen: Str::new("PlayerCrafting"),
            menu_tab: Str::new("Backpack"),
            held: 1 << block,
            player: Entity(512),
            highlighted_recipe: RecipeId(0x7cd8_0a04),
            ..Raw::EMPTY
        });
        // Opening `Input` in init started the reads: the first ask after one
        // interface step answers, with no throwaway ask before it.
        let _drawn = loader.draw_frame();
        let snapshot = input.client_snapshot().unwrap();
        assert_eq!(snapshot.state(), SnapshotState::Fresh);
        assert_eq!(snapshot.session(), 3);
        assert_eq!(snapshot.scene(), Some("world1"));
        assert_eq!(snapshot.hud_screen(), Some("PlayerCrafting"));
        assert_eq!(snapshot.menu_tab(), Some("Backpack"));
        assert_eq!(snapshot.return_screen(), None);
        assert_eq!(snapshot.player(), Some(Entity(512)));
        assert_eq!(snapshot.highlighted_recipe(), Some(RecipeId(0x7cd8_0a04)));
        assert_eq!(snapshot.held().map(|held| held.bits()), Some(1 << block));
    }

    #[test]
    fn a_paused_game_s_snapshot_is_stale_with_its_age() {
        let (loader, input) = input(Side::CLIENT);
        loader.set_client_snapshot(Raw {
            state: Raw::STALE,
            available: Raw::SCREENS | Raw::MENU_TAB,
            session: 1,
            age_ms: 4200,
            hud_screen: Str::new("SystemMenu"),
            ..Raw::EMPTY
        });
        let _drawn = loader.draw_frame();
        let snapshot = input.client_snapshot().unwrap();
        assert_eq!(snapshot.state(), SnapshotState::Stale);
        assert_eq!(snapshot.age_ms(), Some(4200));
        assert_eq!(snapshot.menu_tab(), None, "no menu shows");
        assert!(snapshot.reads(SnapshotPart::MenuTab));
        assert!(!snapshot.reads(SnapshotPart::Held));
    }

    #[test]
    fn a_snapshot_names_the_prompt_s_target_and_verb_and_the_menu_s_character() {
        use enshwick::interactions::Verb;
        let (loader, input) = input(Side::CLIENT);
        loader.set_client_snapshot(Raw {
            state: Raw::FRESH,
            available: Raw::SCREENS | Raw::PROMPT | Raw::CHARACTER,
            session: 1,
            age_ms: 16,
            hud_screen: Str::new("InGame"),
            prompt_target: Entity(1052),
            prompt_verb: Verb::Open.id(),
            ..Raw::EMPTY
        });
        let _drawn = loader.draw_frame();
        let facing = input.client_snapshot().unwrap();
        assert_eq!(facing.prompt_target(), Some(Entity(1052)));
        assert_eq!(
            facing.prompt_verb().and_then(Verb::from_id),
            Some(Verb::Open)
        );
        assert_eq!(facing.selected_character(), None, "no main menu shows");
        assert!(facing.reads(SnapshotPart::Character));
        loader.set_client_snapshot(Raw {
            state: Raw::NEVER,
            available: Raw::PROMPT | Raw::CHARACTER,
            character: 2,
            ..Raw::EMPTY
        });
        let _drawn = loader.draw_frame();
        let menu = input.client_snapshot().unwrap();
        assert_eq!(menu.selected_character(), Some(2));
        assert_eq!((menu.prompt_target(), menu.prompt_verb()), (None, None));
        assert!(menu.reads(SnapshotPart::Prompt));
    }

    #[test]
    fn a_server_has_no_client_snapshot() {
        let (_loader, input) = input(Side::SERVER);
        assert!(matches!(
            input.client_snapshot(),
            Err(WorldError::Unavailable)
        ));
    }
}
