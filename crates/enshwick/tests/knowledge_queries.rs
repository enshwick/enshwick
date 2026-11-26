//! Mods add queries of their own to the game's query database, in the
//! range of their side with every index moved, and name the game's queries
//! by name.

#[cfg(test)]
mod tests {
    use enshwick::abi::Status;
    use enshwick::content::knowledge::{GameQuery, KnowledgeQuery, NewQuery, QueryForm};
    use enshwick::content::{EngineFacts, Id};
    use enshwick::knowledge::{KnowledgeId, Scope};
    use enshwick::resources::{Registry, Value};
    use enshwick::test::{
        FakeHost, QUERY_DATABASE, VANILLA_PLAYER_QUERY, VANILLA_QUERY_NAME,
        VANILLA_WORLD_KNOWLEDGE, VANILLA_WORLD_QUERY,
    };
    use enshwick::{Load, Mod, Off};

    const WORLD: Id = Id::new("you.queries:world");
    const PLAYER: Id = Id::new("you.queries:player");
    const LOST: Id = Id::new("you.queries:lost");

    struct Queries;

    impl Mod for Queries {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let named = load.game_query(VANILLA_QUERY_NAME)?;
            assert_eq!(named, GameQuery::new(VANILLA_PLAYER_QUERY));
            assert!(load.game_query("No_Such_Query").is_err());
            let counted = KnowledgeQuery::at_least(
                KnowledgeId::new(VANILLA_WORLD_KNOWLEDGE),
                Scope::World,
                3,
            );
            let world = load.add_query(NewQuery::new(
                WORLD,
                Scope::World,
                QueryForm::All,
                vec![counted],
            ))?;
            load.add_query({
                let mut value = NewQuery::new(
                    PLAYER,
                    Scope::Player,
                    QueryForm::Any,
                    vec![
                        KnowledgeQuery::game(GameQuery::new(VANILLA_WORLD_QUERY)),
                        KnowledgeQuery::game(world),
                    ],
                );
                value.invert = true;
                value
            })?;
            load.add_query(NewQuery::new(
                LOST,
                Scope::World,
                QueryForm::All,
                vec![KnowledgeQuery::game(GameQuery::new(0x0bad))],
            ))?;
            assert!(
                load.add_query(NewQuery::new(
                    Id::new("you.queries:none"),
                    Scope::World,
                    QueryForm::All,
                    vec![]
                ))
                .is_err()
            );
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Queries,
        r#"
        id = "you.queries"
        side = "both"
        "#
    );

    fn uint(value: &Value, name: &str) -> u64 {
        match value.field(name) {
            Some(Value::Uint(n)) => *n,
            other => panic!("{name}: {other:?}"),
        }
    }

    fn id(query: &Value) -> u64 {
        uint(
            query.field("queryId").unwrap().field("id").unwrap(),
            "value",
        )
    }

    fn part(query: &Value, n: usize) -> (u64, u64) {
        let Some(Value::List(actions)) = query.field("actions") else {
            panic!("{query:?}")
        };
        (
            uint(&actions[n], "queryIndex"),
            uint(&actions[n], "playerProgressionQueryIndex"),
        )
    }

    #[test]
    fn a_mod_s_queries_join_their_side_s_range_with_every_index_in_step() {
        let loader = FakeHost::new().progression().world().install();
        // SAFETY: the three functions are the exports of this file's `declare_mod!(Queries)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        let host = enshwick::host().unwrap();
        let key = QUERY_DATABASE.key_by_hash(host.hash("GameKnowledgeQueryResourceDb").unwrap());
        let base = host.game_resource(key).unwrap();
        let bytes = loader.build_resource(&key, &base).unwrap();
        let db = Registry::game()
            .unwrap()
            .decode("demo::GameKnowledgeQueryResourceDb", &bytes)
            .unwrap();
        assert_eq!(uint(&db, "playerQueryStartIndex"), 2);
        let Some(Value::List(queries)) = db.field("queries") else {
            panic!("{db:?}")
        };
        let ids: Vec<u64> = queries.iter().map(id).collect();
        let key = |id: Id| u64::from(id.guid().fnv1a());
        assert_eq!(
            ids,
            [
                VANILLA_WORLD_QUERY.into(),
                key(WORLD),
                VANILLA_PLAYER_QUERY.into(),
                key(PLAYER)
            ],
            "the lost query is left out"
        );
        for (at, query) in queries.iter().enumerate() {
            assert_eq!(uint(query, "queryIndex"), at as u64);
        }
        assert_eq!(
            part(&queries[2], 0),
            (0, 0),
            "the vanilla part keeps its index"
        );
        assert_eq!(part(&queries[3], 0), (0, 0));
        assert_eq!(
            part(&queries[3], 1),
            (1, 1),
            "a part naming the mod's world query"
        );
        assert_eq!(queries[3].field("invertResult"), Some(&Value::Bool(true)));
        assert_eq!(queries[3].field("type"), Some(&Value::Name("OR".into())));
        assert_eq!(
            queries[1].field("name"),
            Some(&Value::Text(WORLD.to_string()))
        );
        let logs = format!("{:?}", loader.logs());
        assert!(logs.contains("you.queries:lost"), "{logs}");
    }
}
