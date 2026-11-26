//! Mods add knowledge rules to the game's trigger list, change a vanilla
//! rule and turn one off in its place, and read the game's rules.

#[cfg(test)]
mod tests {
    use enshwick::abi::Status;
    use enshwick::content::knowledge::{GameQuery, KnowledgeQuery};
    use enshwick::content::{EngineFacts, Id};
    use enshwick::knowledge::{Grant, KnowledgeId, NewRule, RuleChange, RuleGrant, RuleId, Scope};
    use enshwick::resources::{Registry, Value};
    use enshwick::test::{
        FakeHost, KNOWLEDGE_RULES, VANILLA_PLAYER_KNOWLEDGE, VANILLA_PLAYER_QUERY,
        VANILLA_WORLD_QUERY,
    };
    use enshwick::{Load, Mod, Off};

    const CROWNED: u32 = 0x0000_0c01;

    struct Rules;

    impl Mod for Rules {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let rules = load.knowledge_rules()?;
            assert_eq!(rules.len(), 2, "{rules:?}");
            assert_eq!(
                rules[1].when,
                Some(KnowledgeQuery::game(GameQuery::new(VANILLA_PLAYER_QUERY)))
            );
            assert_eq!(
                rules[1].grant,
                [RuleGrant::new(
                    KnowledgeId::new(VANILLA_PLAYER_KNOWLEDGE),
                    Grant::Set
                )]
            );
            let crown = RuleGrant::new(KnowledgeId::new(CROWNED), Grant::Flag(1));
            load.add_knowledge_rule(NewRule::new(
                Id::new("you.rules:crown"),
                Scope::Player,
                KnowledgeQuery::game(GameQuery::new(VANILLA_WORLD_QUERY)),
                vec![crown],
            ))?;
            assert!(
                load.add_knowledge_rule(NewRule::new(
                    Id::new("you.rules:count"),
                    Scope::World,
                    KnowledgeQuery::game(GameQuery::new(VANILLA_WORLD_QUERY)),
                    vec![RuleGrant::new(KnowledgeId::new(CROWNED), Grant::Add(1))]
                ))
                .is_err(),
                "a counter repeats"
            );
            load.change_knowledge_rule(
                RuleId {
                    scope: Scope::Player,
                    index: 0,
                },
                {
                    let mut value = RuleChange::default();
                    value.grant = Some(vec![crown]);
                    value
                },
            )?;
            load.remove_knowledge_rule(RuleId {
                scope: Scope::World,
                index: 0,
            })?;
            assert!(
                load.remove_knowledge_rule(RuleId {
                    scope: Scope::World,
                    index: 9
                })
                .is_err()
            );
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Rules,
        r#"
        id = "you.rules"
        side = "server"
        "#
    );

    fn uint(value: &Value, path: &[&str]) -> u64 {
        let mut at = value;
        for name in path {
            at = at.field(name).unwrap_or_else(|| panic!("{name} in {at:?}"));
        }
        match at {
            Value::Uint(n) => *n,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn rules_join_the_game_s_list_and_a_removed_one_keeps_its_place() {
        let loader = FakeHost::new().progression().world().install();
        // SAFETY: the three functions are the exports of this file's `declare_mod!(Rules)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        let host = enshwick::host().unwrap();
        let key =
            KNOWLEDGE_RULES.key_by_hash(host.hash("GameKnowledgeQueryTriggerResource").unwrap());
        let base = host.game_resource(key).unwrap();
        let bytes = loader.build_resource(&key, &base).unwrap();
        let triggers = Registry::game()
            .unwrap()
            .decode("demo::GameKnowledgeQueryTriggerResource", &bytes)
            .unwrap();
        let list = |name| match triggers.field(name) {
            Some(Value::List(rules)) => rules.clone(),
            other => panic!("{other:?}"),
        };
        let (world, player) = (list("worldQueries"), list("playerQueries"));
        assert_eq!(world.len(), 1, "a removed rule keeps its place");
        assert_eq!(
            uint(&world[0], &["query", "knowledgeOrQueryId", "value"]),
            0,
            "the game skips query 0"
        );
        assert_eq!(player.len(), 2);
        let Some(Value::List(ops)) = player[0].field("unlockKnowledge") else {
            panic!("{player:?}")
        };
        assert_eq!(
            uint(&ops[0], &["knowledgeId", "id", "value"]),
            u64::from(CROWNED)
        );
        assert_eq!(uint(&ops[0], &["value"]), 2, "flag bit 1");
        assert_eq!(
            uint(&player[1], &["query", "knowledgeOrQueryId", "value"]),
            u64::from(VANILLA_WORLD_QUERY)
        );
    }
}
