//! Mods read the journal's quests, the game's and their own, with their
//! steps and rewards, and where a player and the world stand in each.

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use enshwick::abi::{Header, KnowledgeInfo, Status};
    use enshwick::content::journal::{NewQuest, QuestReward, QuestStep};
    use enshwick::content::knowledge::{GameQuery, KnowledgeQuery, KnowledgeTest, NewKnowledge};
    use enshwick::content::{Guid, Id};
    use enshwick::knowledge::{KnowledgeId, KnowledgeType, Scope};
    use enshwick::quests::{Quests, TextKey};
    use enshwick::resources::fnv1a32;
    use enshwick::test::{FakeHost, PROGRESSION, VANILLA_QUEST};
    use enshwick::{Entity, Load, Mod, Off};

    const TRAIL: Id = Id::new("you.road:trail");
    const MET: Id = Id::new("you.road:met");
    const TORCH: Guid = Guid::parse("c0ffee00-0000-4000-8000-0000000a7c01");
    const PLAYER: Entity = Entity(4096);

    static QUESTS: OnceLock<Quests> = OnceLock::new();
    static EARLY: OnceLock<usize> = OnceLock::new();
    static ENTRIES: OnceLock<(KnowledgeId, KnowledgeId)> = OnceLock::new();

    fn tag(name: &str) -> Guid {
        Guid::named(&format!("you.road:{name}"))
    }

    struct Road;

    impl Mod for Road {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let given = load.add_knowledge(NewKnowledge::new(
                Id::new("you.road:given"),
                Scope::Player,
                KnowledgeType::Bool,
            ))?;
            let wood = load.add_knowledge(NewKnowledge::new(
                Id::new("you.road:wood"),
                Scope::Player,
                KnowledgeType::Counter,
            ))?;
            let is = KnowledgeQuery::is_set(given, Scope::Player);
            load.add_quest(NewQuest {
                rewards: vec![QuestReward::Item(TORCH)],
                ..NewQuest::new(
                    TRAIL,
                    tag("trail.name"),
                    vec![
                        QuestStep::new(
                            tag("1"),
                            tag("1.text"),
                            is,
                            KnowledgeQuery::at_least(wood, Scope::Player, 3),
                        ),
                        QuestStep::new(tag("2"), tag("2.text"), is, is.not().unwrap()),
                    ],
                )
            })?;
            let met = KnowledgeQuery::game(GameQuery::new(77));
            let unmet = KnowledgeQuery::game(GameQuery::new(78)).not().unwrap();
            load.add_quest(NewQuest::new(
                MET,
                tag("met.name"),
                vec![QuestStep::new(tag("m"), tag("m.text"), met, unmet)],
            ))?;
            let _ = ENTRIES.set((given, wood));
            let quests = load.quests();
            // Init cannot read the journal; the failed read must not stick.
            let _ = EARLY.set(quests.all().len());
            let _ = QUESTS.set(quests);
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Road,
        r#"
        id = "you.road"
        side = "both"
        "#
    );

    fn start() -> enshwick::test::Installed {
        let loader = FakeHost::new()
            .progression()
            .world()
            .component(
                PLAYER,
                fnv1a32(PROGRESSION.as_bytes()),
                &[0, 0, 0, 0, 4, 0, 0, 0],
            )
            .install();
        // SAFETY: the three functions are the exports of this file's `declare_mod!(Road)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        loader
    }

    fn known(id: KnowledgeId, kind: u8, value: u32) -> (Option<Entity>, KnowledgeInfo) {
        let info = KnowledgeInfo {
            header: Header::of::<KnowledgeInfo>(),
            id,
            kind,
            scope: KnowledgeInfo::PLAYER,
            reserved: 0,
            value,
            flags: 0,
        };
        (Some(PLAYER), info)
    }

    #[test]
    fn a_mod_s_quest_reads_beside_the_game_s_with_its_steps_and_rewards() {
        let loader = start();
        let quests = QUESTS.get().unwrap();
        let all = quests.all();
        assert_eq!(EARLY.get(), Some(&0), "nothing reads during init");
        assert!(!all.is_empty(), "read after init: {:?}", loader.logs());
        let ids: Vec<KnowledgeId> = all.iter().map(|quest| quest.id).collect();
        let trail = KnowledgeId::from_guid(TRAIL.guid());
        let met = KnowledgeId::from_guid(MET.guid());
        assert_eq!(ids, [KnowledgeId::new(VANILLA_QUEST), trail, met]);
        let quest = quests.quest(trail).unwrap();
        assert_eq!(quest.name, Some(TextKey::of(tag("trail.name"))));
        assert_eq!(quest.rewards, [QuestReward::Item(TORCH)]);
        assert_eq!(quest.steps.len(), 2);
        let (given, wood) = *ENTRIES.get().unwrap();
        assert_eq!(
            quest.steps[0].done_when,
            Some(KnowledgeQuery::at_least(wood, Scope::Player, 3))
        );
        assert_eq!(
            quest.steps[1].done_when.map(|query| (query.id, query.test)),
            Some((given, KnowledgeTest::Equals(0)))
        );
        assert!(quests.quest(KnowledgeId::new(1)).is_none());
    }

    #[test]
    fn a_player_s_state_follows_the_knowledge_its_steps_ask_for() {
        let loader = start();
        let quests = QUESTS.get().unwrap();
        let trail = KnowledgeId::from_guid(TRAIL.guid());
        let (given, wood) = *ENTRIES.get().unwrap();
        loader.set_knowledge(
            &[
                known(given, KnowledgeInfo::BOOL, 0),
                known(wood, KnowledgeInfo::COUNTER, 0),
                known(trail, KnowledgeInfo::BOOL, 0),
            ],
            &[],
        );
        let state = quests.state(PLAYER, trail).unwrap();
        assert!(!state.started && !state.rewarded, "{state:?}");
        assert_eq!(state.steps[0].done, Some(false));
        assert_eq!(state.steps[1].done, Some(true), "asks the bool unset");
        loader.set_knowledge(
            &[
                known(given, KnowledgeInfo::BOOL, 1),
                known(wood, KnowledgeInfo::COUNTER, 3),
                known(trail, KnowledgeInfo::BOOL, 0),
            ],
            &[],
        );
        let state = quests.state(PLAYER, trail).unwrap();
        assert!(state.started, "{state:?}");
        assert_eq!(state.steps[0].done, Some(true));
        assert_eq!(state.steps[1].done, Some(false), "asks the bool unset");
        assert!(!state.done && !state.rewarded);
        let world = quests.world_state(trail).unwrap();
        assert!(
            !world.started,
            "a player's entry reads as not held for the world"
        );
        assert!(quests.state(PLAYER, KnowledgeId::new(1)).is_none());
    }

    #[test]
    fn a_step_that_asks_a_game_query_takes_the_game_s_answer() {
        let loader = start();
        let quests = QUESTS.get().unwrap();
        let met = KnowledgeId::from_guid(MET.guid());
        let step = &quests.quest(met).unwrap().steps[0];
        assert_eq!(step.shown_when.map(|q| q.test), Some(KnowledgeTest::Holds));
        assert_eq!(step.done_when.map(|q| q.test), Some(KnowledgeTest::Fails));
        loader.set_game_queries(&[
            (Some(PLAYER), 77, true),
            (Some(PLAYER), 78, false),
            (None, 77, false),
        ]);
        let state = quests.state(PLAYER, met).unwrap();
        assert_eq!(state.steps[0].shown, Some(true));
        assert_eq!(state.steps[0].done, Some(true), "78 does not hold");
        assert!(state.started && state.done, "{state:?}");
        let world = quests.world_state(met).unwrap();
        assert_eq!(world.steps[0].shown, Some(false));
        assert_eq!(world.steps[0].done, None, "the game has no answer");
    }
}
