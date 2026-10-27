//! Knowledge: reads, grants and quests, as the loader receives them.

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use enshwick::Entity;
    use enshwick::abi::{Header, KnowledgeData, KnowledgeInfo, Status};
    use enshwick::events::KnowledgeChanged;
    use enshwick::knowledge::{Grant, Knowledge, KnowledgeId, KnowledgeType, Scope};
    use enshwick::world::WorldError;
    use enshwick::{Load, Mod, Off};

    static KNOWLEDGE: OnceLock<Knowledge> = OnceLock::new();

    const RECIPE: KnowledgeId = KnowledgeId::new(0x1d);
    const QUEST: KnowledgeId = KnowledgeId::new(0x51);
    const OPEN_QUEST: KnowledgeId = KnowledgeId::new(0x52);
    const FOUND: KnowledgeId = KnowledgeId::new(0x77);
    const UNKNOWN: KnowledgeId = KnowledgeId::new(0x99);

    struct Scholar;

    impl Mod for Scholar {
        fn manifest() -> &'static enshwick::abi::Manifest {
            enshwick::manifest!(id: "you.scholar", side: SERVER)
        }

        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let _first = KNOWLEDGE.set(load.knowledge());
            load.on::<KnowledgeChanged>(|changed| {
                enshwick::info!("{} of {} is {}", changed.id, changed.entity, changed.value);
            })?;
            Ok(Self)
        }
    }

    enshwick::declare_mod!(Scholar);

    fn info(id: KnowledgeId, kind: u8, scope: u8, value: u32, flags: u32) -> KnowledgeInfo {
        KnowledgeInfo {
            header: Header::of::<KnowledgeInfo>(),
            id,
            kind,
            scope,
            reserved: 0,
            value,
            flags,
        }
    }

    #[test]
    fn knowledge_reads_grants_and_quests() {
        let loader = enshwick::test::FakeHost::new().install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Scholar)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let knowledge = KNOWLEDGE.get().unwrap();

        let recipe = info(RECIPE, KnowledgeInfo::FLAGS, KnowledgeInfo::PLAYER, 1, 3);
        let quest = info(QUEST, KnowledgeInfo::BOOL, KnowledgeInfo::PLAYER, 1, 0);
        let found = info(FOUND, KnowledgeInfo::COUNTER, KnowledgeInfo::WORLD, 4, 0);
        let player = Some(Entity(2));
        loader.set_knowledge(
            &[(player, recipe), (player, quest), (None, found)],
            &[QUEST, OPEN_QUEST],
        );

        let known = knowledge.get(Entity(2), RECIPE).unwrap();
        assert_eq!(
            (known.r#type, known.scope),
            (KnowledgeType::Flags, Scope::Player)
        );
        assert!(known.has_flag(0) && !known.has_flag(1));
        assert_eq!(knowledge.get(Entity(3), RECIPE), None, "another player's");
        assert_eq!(knowledge.world(FOUND).map(|k| k.value), Some(4));
        assert_eq!(
            knowledge.get(Entity(3), FOUND).map(|k| k.value),
            Some(4),
            "the world's for anyone"
        );

        knowledge.grant(Entity(2), RECIPE, Grant::Flag(1)).unwrap();
        knowledge.grant_world(FOUND, Grant::Add(2)).unwrap();
        assert_eq!(
            knowledge.grant(Entity(2), RECIPE, Grant::Set),
            Err(WorldError::Refused)
        );
        assert_eq!(
            knowledge.grant(Entity(2), UNKNOWN, Grant::Set),
            Err(WorldError::Refused)
        );
        assert_eq!(
            loader.grants(),
            [
                (player, RECIPE, u32::from(KnowledgeInfo::FLAGS), 1),
                (None, FOUND, u32::from(KnowledgeInfo::COUNTER), 2)
            ]
        );

        assert_eq!(knowledge.quests(), [QUEST, OPEN_QUEST]);
        assert!(knowledge.completed(Entity(2), QUEST));
        assert!(!knowledge.completed(Entity(2), OPEN_QUEST));

        let changed = KnowledgeData {
            header: Header::of::<KnowledgeData>(),
            entity: Entity(2),
            id: RECIPE,
            value: 3,
        };
        assert_eq!(loader.raise::<KnowledgeChanged>(&changed), 1);
    }
}
