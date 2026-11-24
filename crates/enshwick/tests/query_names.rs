//! A mod that only names the game's queries serves no patch of the query
//! database.

#[cfg(test)]
mod tests {
    use enshwick::abi::Status;
    use enshwick::content::EngineFacts;
    use enshwick::content::knowledge::GameQuery;
    use enshwick::test::{FakeHost, QUERY_DATABASE, VANILLA_PLAYER_QUERY, VANILLA_QUERY_NAME};
    use enshwick::{Load, Mod, Off};

    struct Names;

    impl Mod for Names {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            assert_eq!(
                load.game_query(VANILLA_QUERY_NAME)?,
                GameQuery::new(VANILLA_PLAYER_QUERY)
            );
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Names,
        r#"
        id = "you.names"
        side = "both"
        "#
    );

    #[test]
    fn naming_a_game_query_serves_no_patch_of_the_database() {
        let loader = FakeHost::new().progression().world().install();
        // SAFETY: the three functions are the exports of this file's `declare_mod!(Names)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        let host = enshwick::host().unwrap();
        let key = QUERY_DATABASE.key_by_hash(host.hash("GameKnowledgeQueryResourceDb").unwrap());
        let base = host.game_resource(key).unwrap();
        assert_eq!(loader.build_resource(&key, &base), Err(Status::MISSING));
    }
}
