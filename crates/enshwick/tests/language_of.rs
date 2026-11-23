//! The server reads each player's language, as their game named it when
//! they joined, for texts outside an action's request.

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use enshwick::abi::{Side, Status};
    use enshwick::i18n::{Language, Texts};
    use enshwick::world::World;
    use enshwick::{Load, Mod, Off, PlayerKey};

    static WORLD: OnceLock<World> = OnceLock::new();

    struct Notices;

    impl Mod for Notices {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let _first = WORLD.set(load.world()?);
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Notices,
        r#"
        id = "you.notices"
        side = "server"
        "#
    );

    #[test]
    fn a_player_s_language_fills_a_text_for_that_player_and_others_read_the_fallback() {
        let loader = enshwick::test::FakeHost::new()
            .world()
            .player_language(PlayerKey::new(7), Language::UK_UA)
            .install();
        // SAFETY: the three functions are the exports of this file's `declare_mod!`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        let world = WORLD.get().unwrap();
        let texts =
            Texts::parse("[en]\nrestart = \"Restart\"\n[uk]\nrestart = \"Перезапуск\"\n").unwrap();

        assert_eq!(world.language_of(PlayerKey::new(7)), Language::UK_UA);
        assert_eq!(
            texts.fill("restart", world.language_of(PlayerKey::new(7)), &[]),
            "Перезапуск"
        );
        assert_eq!(
            world.language_of(PlayerKey::new(8)),
            Language::FALLBACK,
            "a game without Enshwick"
        );
        assert_eq!(
            texts.fill("restart", world.language_of(PlayerKey::new(8)), &[]),
            "Restart"
        );
    }

    #[test]
    fn the_host_of_a_game_hosted_from_a_client_reads_the_game_s_own_language() {
        let loader = enshwick::test::FakeHost::new()
            .world()
            .side(Side::CLIENT)
            .hosting(true)
            .host_key(PlayerKey::new(7))
            .language(Language::UK_UA)
            .player_language(PlayerKey::new(8), Language::DE_DE)
            .install();
        // SAFETY: the three functions are the exports of this file's `declare_mod!`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        let world = WORLD.get().unwrap();

        assert_eq!(world.language_of(PlayerKey::new(7)), Language::UK_UA);
        assert_eq!(
            world.language_of(PlayerKey::new(8)),
            Language::DE_DE,
            "a guest keeps the language its game named"
        );
        assert_eq!(world.language_of(PlayerKey::new(9)), Language::FALLBACK);
    }
}
