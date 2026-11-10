//! The content example `examples/you.trail`, loaded as the loader loads a
//! pack with a script: its knowledge, quest, lore page, skill node and the
//! rewards that move the quest on.

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use enshwick::abi::Status;
    use enshwick::content::journal::JOURNAL_REGISTRY;
    use enshwick::content::knowledge::GAME_KNOWLEDGE;
    use enshwick::content::{EngineFacts, Id};
    use enshwick::packs::{Pack, PackFiles, PackManifest};
    use enshwick::test::FakeHost;
    use enshwick::{Load, Mod, Off};
    use enshwick_luau::{Limits, Scripts};

    const ID: &str = "you.trail";

    static RUNTIME: Mutex<Option<enshwick_luau::Runtime>> = Mutex::new(None);

    fn read(name: &str) -> String {
        let dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../examples/you.trail");
        std::fs::read_to_string(dir.join(name)).unwrap()
    }

    struct Trail;

    impl Mod for Trail {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let mut files = PackFiles::new();
            for name in ["pack.toml", "rewards.toml", "main.luau"] {
                files = files.with(name, read(name));
            }
            let pack = Pack::parse_with(ID, &files, None).map_err(|e| Off::new(e.to_string()))?;
            load.add_pack(&pack)?;
            let manifest =
                PackManifest::parse(&read("pack.toml")).map_err(|e| Off::new(e.to_string()))?;
            let scripts = Scripts::new().with("main.luau", read("main.luau"));
            let runtime = enshwick_luau::start_pack(load, &manifest, scripts, Limits::default())?;
            *RUNTIME.lock().unwrap() = Some(runtime);
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

    #[test]
    fn the_trail_adds_its_quest_page_and_a_node_beside_the_first_root() {
        let loader = FakeHost::new().progression().items().install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Trail)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        let logs = format!("{:?}", loader.logs());
        assert_eq!(status, Status::OK, "{logs}");
        let node = Id::new("you.trail:endurance").guid().fnv1a();
        let logged = |text: &str| loader.logs().iter().any(|(_, line)| line.contains(text));
        assert!(logged(&format!("node {node} copies")), "{logs}");
        let host = FakeHost::new().progression();
        let journal = JOURNAL_REGISTRY.key_by_hash(host.hash("JournalRegistryResource").unwrap());
        let knowledge = GAME_KNOWLEDGE.key_by_hash(host.hash("GameKnowledgeResource").unwrap());
        for key in [journal, knowledge] {
            assert!(loader.resources().iter().any(|r| r.key == key), "{logs}");
        }
        assert!(loader.faults().is_empty(), "{:?}", loader.faults());
    }

    #[test]
    fn the_trail_gives_its_torch_when_its_quest_completes_and_no_other() {
        use enshwick::abi::{
            Header, InventoryOpResult, InventoryRequest, InventoryResult, QuestData,
        };
        use enshwick::events::QuestCompleted;
        use enshwick::knowledge::KnowledgeId;

        static SKIPPED: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(44);
        let loader = FakeHost::new()
            .symbol("ecs.type_index", 0x1_4064_5cc0)
            .symbol("ecs.component_read_raw", 0x1_4064_6b00)
            .symbol("ecs.slot_root", 0x1_4093_5420)
            .symbol("inventory.skip_entity", SKIPPED.as_ptr().addr() as u64)
            .constant("ecs.context_entity_world", 0x30)
            .constant("ecs.registry_type_map", 0x30)
            .constant("ecs.registry_type_map_fallback", 0x88)
            .progression()
            .items()
            .world()
            .inventory()
            .install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Trail)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        let completed = |quest| QuestData {
            header: Header::of::<QuestData>(),
            entity: enshwick::Entity(2),
            quest,
        };
        let vanilla = KnowledgeId::new(enshwick::test::VANILLA_QUEST);
        assert_eq!(loader.raise::<QuestCompleted>(&completed(vanilla)), 1);
        assert!(loader.inventory_requests().is_empty(), "not the trail");

        let trail = KnowledgeId::from_guid(Id::new("you.trail:trail").guid());
        assert_eq!(loader.raise::<QuestCompleted>(&completed(trail)), 1);
        let requests = loader.inventory_requests();
        assert_eq!(requests.len(), 1, "{:?}", loader.logs());
        assert_ne!(requests[0].flags & InventoryRequest::ALLOW_PARTIAL, 0);
        let torch = enshwick::content::Guid::parse("224097e5-ed0d-49d2-9d23-43256a6682b3");
        assert_eq!(
            (
                requests[0].ops[0].target,
                requests[0].ops[0].item,
                requests[0].ops[0].count
            ),
            (2, enshwick::ItemId::from_guid(torch), 1)
        );
        let none_fit = InventoryOpResult {
            error: 0,
            remaining: 1,
        };
        assert!(loader.finish_inventory(InventoryResult::COMMITTED, &[none_fit]));
        assert!(loader.faults().is_empty(), "{:?}", loader.faults());
    }

    #[test]
    fn the_rewards_name_the_knowledge_the_script_adds() {
        let rewards = read("rewards.toml");
        for entry in ["you.trail:given", "you.trail:wood", "you.trail:resin"] {
            let guid = Id::new(entry).guid().to_string();
            assert!(rewards.contains(&guid), "{entry}: {guid}");
        }
    }
}
