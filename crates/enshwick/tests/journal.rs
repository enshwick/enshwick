//! Mods add quests and lore pages to the journal on their own knowledge,
//! change a vanilla quest's rewards and steps in load order, and set a
//! player's quest progression.

#[cfg(test)]
mod tests {
    use std::sync::{Mutex, OnceLock};

    use enshwick::abi::Status;
    use enshwick::content::journal::{
        JOURNAL_REGISTRY, NewJournalPage, NewQuest, PageEntry, QuestChange, QuestReward, QuestStep,
        StepChange,
    };
    use enshwick::content::knowledge::{GAME_KNOWLEDGE, KnowledgeQuery, NewKnowledge};
    use enshwick::content::{EngineFacts, Guid, Id};
    use enshwick::knowledge::{Knowledge, KnowledgeId, KnowledgeType, Scope};
    use enshwick::resources::{Registry, Value, fnv1a32};
    use enshwick::test::{FakeHost, PROGRESSION, QUEST_ITEM, VANILLA_QUEST};
    use enshwick::{Entity, Load, Mod, Off};

    const TRAIL: Id = Id::new("you.trail:trail");
    const LORE: Id = Id::new("you.trail:lore");
    const TORCH: Guid = Guid::parse("c0ffee00-0000-4000-8000-0000000a7c01");
    const PLAYER: Entity = Entity(4096);

    static QUEST: Mutex<Option<KnowledgeId>> = Mutex::new(None);
    static KNOWLEDGE: OnceLock<Knowledge> = OnceLock::new();
    static REFUSED: Mutex<Vec<String>> = Mutex::new(Vec::new());

    fn refused(result: Result<impl Sized, Off>) {
        if let Err(off) = result {
            REFUSED.lock().unwrap().push(off.to_string());
        }
    }

    fn tag(name: &str) -> Guid {
        Guid::named(name)
    }

    struct Trail;

    impl Mod for Trail {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let given = load.add_knowledge(NewKnowledge::new(
                Id::new("you.trail:given"),
                Scope::Player,
                KnowledgeType::Bool,
            ))?;
            let wood = load.add_knowledge(NewKnowledge::new(
                Id::new("you.trail:wood"),
                Scope::Player,
                KnowledgeType::Counter,
            ))?;
            let is = |id| KnowledgeQuery::is_set(id, Scope::Player);
            let quest = load.add_quest(NewQuest {
                rewards: vec![QuestReward::Item(TORCH)],
                ..NewQuest::new(
                    TRAIL,
                    tag("trail.name"),
                    vec![
                        QuestStep::new(
                            tag("trail.1"),
                            tag("trail.1.text"),
                            is(given),
                            KnowledgeQuery::at_least(wood, Scope::Player, 3),
                        ),
                        QuestStep::new(tag("trail.2"), tag("trail.2.text"), is(given), is(given)),
                    ],
                )
            })?;
            *QUEST.lock().unwrap() = Some(quest);
            load.add_journal_page(NewJournalPage {
                id: LORE,
                category: tag("lore.category"),
                name: tag("lore.name"),
                entries: vec![PageEntry {
                    name: tag("lore.1"),
                    text: tag("lore.1.text"),
                    shown_when: is(given),
                }],
            })?;
            let vanilla = KnowledgeId::new(VANILLA_QUEST);
            load.change_quest(
                vanilla,
                QuestChange {
                    rewards: Some(Vec::new()),
                    for_all_players: Some(true),
                    ..QuestChange::default()
                },
            )?;
            load.change_quest(
                vanilla,
                QuestChange {
                    add_rewards: vec![QuestReward::Item(TORCH)],
                    steps: vec![StepChange {
                        step: 0,
                        shown_when: None,
                        done_when: Some(is(given)),
                    }],
                    ..QuestChange::default()
                },
            )?;
            refused(load.add_quest(NewQuest::new(
                Id::new("you.trail:empty"),
                tag("empty"),
                Vec::new(),
            )));
            refused(load.change_quest(KnowledgeId::new(1), QuestChange::default()));
            refused(load.change_quest(
                vanilla,
                QuestChange {
                    steps: vec![StepChange {
                        step: 4,
                        shown_when: Some(is(given)),
                        done_when: None,
                    }],
                    ..QuestChange::default()
                },
            ));
            refused(load.add_quest(NewQuest::new(
                Id::new("you.trail:zero"),
                tag("zero"),
                vec![QuestStep::new(
                    tag("z"),
                    tag("z.text"),
                    is(given),
                    KnowledgeQuery::at_least(wood, Scope::Player, 0),
                )],
            )));
            let _ = KNOWLEDGE.set(load.knowledge());
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Trail,
        r#"
        id = "you.trail"
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
        // SAFETY: the three functions are the exports of this file's `declare_mod!(Trail)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        loader
    }

    fn built(loader: &enshwick::test::Installed, guid: Guid, id: &str, ty: &str) -> Value {
        let host = enshwick::host().unwrap();
        let key = guid.key_by_hash(host.hash(id).unwrap());
        let base = host.game_resource(key).unwrap();
        let bytes = loader.build_resource(&key, &base).unwrap();
        Registry::game().unwrap().decode(ty, &bytes).unwrap()
    }

    fn key(value: Option<&Value>) -> Option<u32> {
        match value.and_then(|v| v.field("value")) {
            Some(Value::Uint(n)) => u32::try_from(*n).ok(),
            _ => None,
        }
    }

    fn list(value: Option<&Value>) -> &[Value] {
        match value {
            Some(Value::List(items)) => items,
            _ => &[],
        }
    }

    #[test]
    fn a_quest_and_a_page_join_the_journal_and_the_quest_id_is_a_player_s_bool() {
        let loader = start();
        let journal = built(
            &loader,
            JOURNAL_REGISTRY,
            "JournalRegistryResource",
            "demo::JournalRegistryResource",
        );
        let quest = QUEST.lock().unwrap().unwrap();
        let quests = list(journal.field("quests"));
        assert_eq!(quests.len(), 2, "beside the vanilla quest");
        let trail = &quests[1];
        assert_eq!(key(trail.field("entryId")), Some(quest.get()));
        assert_eq!(key(trail.field("name")), Some(tag("trail.name").fnv1a()));
        assert_eq!(
            key(trail.field("referencedDocumentName")),
            key(quests[0].field("referencedDocumentName")),
            "the game's document"
        );
        assert_eq!(list(trail.field("entries")).len(), 2);
        let done = list(trail.field("entries"))[0]
            .field("completionRequirement")
            .unwrap();
        assert_eq!(done.field("type"), Some(&Value::Name("SimpleCount".into())));
        assert_eq!(done.field("compareValue"), Some(&Value::Uint(2)));
        assert_eq!(
            list(trail.field("rewards"))[0].field("itemRef"),
            Some(&Value::Guid(TORCH))
        );
        let page = &list(journal.field("collections"))[0];
        assert_eq!(
            key(page.field("loreCategory")),
            Some(tag("lore.category").fnv1a())
        );

        let knowledge = built(
            &loader,
            GAME_KNOWLEDGE,
            "GameKnowledgeResource",
            "demo::GameKnowledgeResource",
        );
        let players = list(
            knowledge
                .field("playerKnowledge")
                .and_then(|m| m.field("keys")),
        );
        assert!(
            players
                .iter()
                .any(|k| key(k.field("id")) == Some(quest.get())),
            "the quest's bool"
        );

        let refused = REFUSED.lock().unwrap().clone();
        assert_eq!(refused.len(), 4, "{refused:?}");
        assert!(refused[0].contains("no steps"), "{}", refused[0]);
        assert!(refused[1].contains("no quest"), "{}", refused[1]);
        assert!(refused[2].contains("no step 4"), "{}", refused[2]);
        assert!(refused[3].contains("start at 1"), "{}", refused[3]);
    }

    #[test]
    fn two_changes_of_a_vanilla_quest_chain_in_load_order() {
        let loader = start();
        let journal = built(
            &loader,
            JOURNAL_REGISTRY,
            "JournalRegistryResource",
            "demo::JournalRegistryResource",
        );
        let vanilla = &list(journal.field("quests"))[0];
        assert_eq!(key(vanilla.field("entryId")), Some(VANILLA_QUEST));
        let rewards = list(vanilla.field("rewards"));
        assert_eq!(rewards.len(), 1, "the first change cleared them");
        assert_eq!(rewards[0].field("itemRef"), Some(&Value::Guid(TORCH)));
        assert_ne!(rewards[0].field("itemRef"), Some(&Value::Guid(QUEST_ITEM)));
        assert_eq!(
            vanilla.field("unlockForAllPlayers"),
            Some(&Value::Bool(true))
        );
        let step = &list(vanilla.field("entries"))[0];
        let done = step.field("completionRequirement").unwrap();
        assert_eq!(done.field("type"), Some(&Value::Name("SimpleBool".into())));
    }

    #[test]
    fn a_player_s_quest_progression_is_read_and_set_with_its_change_counted() {
        let loader = start();
        let knowledge = KNOWLEDGE.get().unwrap();
        assert_eq!(knowledge.quest_progression(PLAYER), Some(false));
        knowledge.set_quest_progression(PLAYER, true).unwrap();
        assert_eq!(knowledge.quest_progression(PLAYER), Some(true));
        assert_eq!(
            loader.component_bytes(PLAYER, fnv1a32(PROGRESSION.as_bytes())),
            Some(vec![1, 0, 0, 0, 5, 0, 0, 0])
        );
        assert_eq!(knowledge.quest_progression(Entity(1)), None);
        assert!(knowledge.set_quest_progression(Entity(1), true).is_err());
    }
}
