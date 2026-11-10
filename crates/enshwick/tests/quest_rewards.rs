//! A mod hands out its own quest's item reward when the quest completes,
//! since the game gives a quest's experience and never its items: the
//! handler reads the quest's declared rewards, gives what fits and hears
//! how many did not.

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use enshwick::abi::{
        Header, InventoryOp, InventoryOpResult, InventoryRequest, InventoryResult, QuestData,
        Status,
    };
    use enshwick::content::journal::{NewQuest, QuestReward, QuestStep};
    use enshwick::content::knowledge::{ExperienceValue, KnowledgeQuery, NewKnowledge};
    use enshwick::content::{Guid, Id};
    use enshwick::events::QuestCompleted;
    use enshwick::knowledge::{KnowledgeId, KnowledgeType, Scope};
    use enshwick::test::{FakeHost, VANILLA_QUEST, VANILLA_QUEST_EXPERIENCE};
    use enshwick::{Entity, ItemId, Load, Mod, Off};

    const TORCH: Guid = Guid::parse("224097e5-ed0d-49d2-9d23-43256a6682b3");
    const PLAYER: Entity = Entity(2);

    static QUEST: Mutex<Option<KnowledgeId>> = Mutex::new(None);
    static LEFT: Mutex<Vec<u32>> = Mutex::new(Vec::new());
    static REWARDS: std::sync::OnceLock<enshwick::rewards::Rewards> = std::sync::OnceLock::new();
    static SKIPPED: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(44);

    struct Trail;

    impl Mod for Trail {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let given = load.add_knowledge(NewKnowledge::new(
                Id::new("you.trail:given"),
                Scope::Player,
                KnowledgeType::Bool,
            ))?;
            let is_given = KnowledgeQuery::is_set(given, Scope::Player);
            let tag = |name: &str| Guid::named(&format!("you.trail:{name}"));
            let trail = load.add_quest(NewQuest {
                rewards: vec![
                    QuestReward::Experience(ExperienceValue::new(VANILLA_QUEST_EXPERIENCE)),
                    QuestReward::Item(TORCH),
                ],
                ..NewQuest::new(
                    Id::new("you.trail:trail"),
                    tag("trail.name"),
                    vec![QuestStep::new(
                        tag("step"),
                        tag("step.text"),
                        is_given,
                        is_given,
                    )],
                )
            })?;
            *QUEST.lock().unwrap() = Some(trail);
            let _set = REWARDS.set(load.rewards()?);
            let quests = load.quests();
            let inventories = load.inventories()?;
            load.on::<QuestCompleted>(move |done| {
                if done.quest != trail {
                    return;
                }
                let rewards = quests
                    .quest(trail)
                    .map_or_else(Vec::new, |quest| quest.rewards);
                for reward in rewards {
                    if let QuestReward::Item(item) = reward {
                        let _ticket = inventories.give_what_fits(
                            done.entity,
                            ItemId::from_guid(item),
                            2,
                            |left| LEFT.lock().unwrap().push(left),
                        );
                    }
                }
            })?;
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

    fn completed(quest: KnowledgeId) -> QuestData {
        QuestData {
            header: Header::of::<QuestData>(),
            entity: PLAYER,
            quest,
        }
    }

    fn start(name: &str) -> enshwick::test::Installed {
        let data = std::env::temp_dir().join(format!("enshwick-{name}-{}", std::process::id()));
        let _gone = std::fs::remove_dir_all(&data);
        let loader = FakeHost::new()
            .symbol("ecs.type_index", 0x1_4064_5cc0)
            .symbol("ecs.component_read_raw", 0x1_4064_6b00)
            .symbol("ecs.slot_root", 0x1_4093_5420)
            .symbol("inventory.skip_entity", SKIPPED.as_ptr().addr() as u64)
            .constant("ecs.context_entity_world", 0x30)
            .constant("ecs.registry_type_map", 0x30)
            .constant("ecs.registry_type_map_fallback", 0x88)
            .progression()
            .world()
            .inventory()
            .data_dir(data.to_str().unwrap())
            .world_key("world0")
            .install();
        // SAFETY: the three functions are the exports of this file's `declare_mod!(Trail)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        loader
    }

    #[test]
    fn a_reward_s_items_go_in_as_far_as_they_fit_and_the_log_names_the_rest() {
        let loader = start("reward-items");
        let item = ItemId::from_guid(TORCH);
        let give = [enshwick::rewards::Give::Items { item, count: 3 }];
        let short = |loader: &enshwick::test::Installed| {
            loader
                .logs()
                .iter()
                .filter(|(_, line)| line.contains("did not fit"))
                .count()
        };
        let fit = |remaining| InventoryOpResult {
            error: 0,
            remaining,
        };

        REWARDS.get().unwrap().give(PLAYER, &give);
        let requests = loader.inventory_requests();
        assert_eq!(requests.len(), 1);
        assert_ne!(requests[0].flags & InventoryRequest::ALLOW_PARTIAL, 0);
        assert!(loader.finish_inventory(InventoryResult::COMMITTED, &[fit(0)]));
        assert_eq!(short(&loader), 0, "all in, nothing to say");

        REWARDS.get().unwrap().give(PLAYER, &give);
        assert!(loader.finish_inventory(InventoryResult::COMMITTED, &[fit(2)]));
        assert_eq!(short(&loader), 1, "{:?}", loader.logs());
        assert!(
            loader
                .logs()
                .iter()
                .any(|(_, line)| line.contains("2 of 3 items") && line.contains("player 2")),
            "{:?}",
            loader.logs()
        );
    }

    #[test]
    fn a_mod_gives_its_quest_s_item_on_completion_and_hears_what_did_not_fit() {
        let loader = start("quest-items");
        let trail = QUEST.lock().unwrap().unwrap();

        assert_eq!(
            loader.raise::<QuestCompleted>(&completed(KnowledgeId::new(VANILLA_QUEST))),
            1
        );
        assert!(
            loader.inventory_requests().is_empty(),
            "a quest of the game's gives nothing"
        );

        assert_eq!(loader.raise::<QuestCompleted>(&completed(trail)), 1);
        let requests = loader.inventory_requests();
        assert_eq!(requests.len(), 1, "the item, not the experience");
        assert_eq!(
            requests[0].flags & InventoryRequest::ALLOW_PARTIAL,
            InventoryRequest::ALLOW_PARTIAL
        );
        let op = requests[0].ops[0];
        assert_eq!(
            (op.kind, op.target, op.item, op.count),
            (
                InventoryOp::GIVE,
                u64::from(PLAYER.0),
                ItemId::from_guid(TORCH),
                2
            )
        );
        let fit = |remaining| InventoryOpResult {
            error: 0,
            remaining,
        };
        assert!(loader.finish_inventory(InventoryResult::COMMITTED, &[fit(1)]));

        assert_eq!(loader.raise::<QuestCompleted>(&completed(trail)), 1);
        assert!(loader.finish_inventory(InventoryResult::COMMITTED, &[fit(0)]));
        assert_eq!(loader.raise::<QuestCompleted>(&completed(trail)), 1);
        assert!(loader.finish_inventory(InventoryResult::TIMED_OUT, &[]));
        assert_eq!(loader.raise::<QuestCompleted>(&completed(trail)), 1);
        assert!(loader.finish_inventory(InventoryResult::ROLLED_BACK, &[fit(1)]));
        assert_eq!(
            *LEFT.lock().unwrap(),
            [1, 0, 2, 2],
            "one short, all in, and all of them when nothing changed"
        );
    }
}
