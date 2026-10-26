//! Knowledge: reads, grants and quests, as the loader receives them.

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use enshwick::abi::{Header, KnowledgeData, KnowledgeInfo, Status};
    use enshwick::ecs::Entity;
    use enshwick::events::KnowledgeChanged;
    use enshwick::knowledge::{Grant, Knowledge, KnowledgeKind, Scope};
    use enshwick::world::WorldError;
    use enshwick::{Load, Mod, Off};

    static KNOWLEDGE: OnceLock<Knowledge> = OnceLock::new();

    struct Scholar;

    impl Mod for Scholar {
        fn manifest() -> &'static enshwick::abi::Manifest {
            enshwick::manifest!(id: "scholar", side: SERVER)
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

    fn info(id: u32, kind: u8, scope: u8, value: u32, flags: u32) -> KnowledgeInfo {
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
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let knowledge = KNOWLEDGE.get().unwrap();

        let recipe = info(0x1d, KnowledgeInfo::FLAGS, KnowledgeInfo::PLAYER, 1, 3);
        let quest = info(0x51, KnowledgeInfo::BOOL, KnowledgeInfo::PLAYER, 1, 0);
        let found = info(0x77, KnowledgeInfo::COUNTER, KnowledgeInfo::WORLD, 4, 0);
        loader.set_knowledge(&[(2, recipe), (2, quest), (0, found)], &[0x51, 0x52]);

        let known = knowledge.get(Entity(2), 0x1d).unwrap();
        assert_eq!(
            (known.kind, known.scope),
            (KnowledgeKind::Flags, Scope::Player)
        );
        assert!(known.has_flag(0) && !known.has_flag(1));
        assert_eq!(knowledge.get(Entity(3), 0x1d), None, "another player's");
        assert_eq!(knowledge.world(0x77).map(|k| k.value), Some(4));
        assert_eq!(
            knowledge.get(Entity(3), 0x77).map(|k| k.value),
            Some(4),
            "the world's for anyone"
        );

        knowledge.grant(Entity(2), 0x1d, Grant::Flag(1)).unwrap();
        knowledge.grant_world(0x77, Grant::Add(2)).unwrap();
        assert_eq!(
            knowledge.grant(Entity(2), 0x1d, Grant::Set),
            Err(WorldError::Refused)
        );
        assert_eq!(
            knowledge.grant(Entity(2), 0x99, Grant::Set),
            Err(WorldError::Refused)
        );
        assert_eq!(
            loader.grants(),
            [
                (2, 0x1d, u32::from(KnowledgeInfo::FLAGS), 1),
                (0, 0x77, u32::from(KnowledgeInfo::COUNTER), 2)
            ]
        );

        assert_eq!(knowledge.quests(), [0x51, 0x52]);
        assert!(knowledge.completed(Entity(2), 0x51));
        assert!(!knowledge.completed(Entity(2), 0x52));

        let changed = KnowledgeData {
            header: Header::of::<KnowledgeData>(),
            entity: 2,
            id: 0x1d,
            value: 3,
        };
        assert_eq!(loader.raise::<KnowledgeChanged>(&changed), 1);
    }
}
