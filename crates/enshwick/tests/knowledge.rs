//! Knowledge: reads, grants and quests, as the loader receives them.

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use enshwick::Entity;
    use enshwick::WorldError;
    use enshwick::abi::{KnowledgeChange, KnowledgeData, KnowledgeInfo, Status};
    use enshwick::events::KnowledgeChanged;
    use enshwick::knowledge::{Grant, Knowledge, KnowledgeId, KnowledgeType, Scope};
    use enshwick::{Load, Mod, Off};

    static KNOWLEDGE: OnceLock<Knowledge> = OnceLock::new();

    const RECIPE: KnowledgeId = KnowledgeId::new(0x1d);
    const QUEST: KnowledgeId = KnowledgeId::new(0x51);
    const OPEN_QUEST: KnowledgeId = KnowledgeId::new(0x52);
    const FOUND: KnowledgeId = KnowledgeId::new(0x77);
    const UNKNOWN: KnowledgeId = KnowledgeId::new(0x99);

    struct Scholar;

    impl Mod for Scholar {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let _first = KNOWLEDGE.set(load.knowledge());
            load.on::<KnowledgeChanged>(|changed| {
                enshwick::info!("{} of {} is {}", changed.id, changed.entity, changed.value);
            })?;
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Scholar,
        r#"
        id = "you.scholar"
        side = "server"
        "#
    );

    fn info(id: KnowledgeId, kind: u8, scope: u8, value: u32, flags: u32) -> KnowledgeInfo {
        enshwick::abi::headed!(KnowledgeInfo {
            id,
            kind,
            scope,
            reserved: 0,
            value,
            flags,
        })
    }

    #[test]
    fn knowledge_reads_grants_and_quests() {
        let loader = enshwick::test::FakeHost::new().install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Scholar)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let knowledge = KNOWLEDGE.get().unwrap();

        let recipe = info(RECIPE, KnowledgeInfo::FLAGS, KnowledgeInfo::PLAYER, 1, 3);
        let quest = info(QUEST, KnowledgeInfo::BOOL, KnowledgeInfo::PLAYER, 1, 0);
        let found = info(FOUND, KnowledgeInfo::COUNTER, KnowledgeInfo::WORLD, 4, 0);
        let player = Some(Entity::from_engine(2));
        loader.set_knowledge(
            &[(player, recipe), (player, quest), (None, found)],
            &[QUEST, OPEN_QUEST],
        );

        let known = knowledge.get(Entity::from_engine(2), RECIPE).unwrap();
        assert_eq!(
            (known.r#type, known.scope),
            (KnowledgeType::Flags, Scope::Player)
        );
        assert!(known.has_flag(0) && !known.has_flag(1));
        assert_eq!(
            knowledge.get(Entity::from_engine(3), RECIPE),
            None,
            "another player's"
        );
        assert_eq!(knowledge.world(FOUND).map(|k| k.value), Some(4));
        assert_eq!(
            knowledge
                .get(Entity::from_engine(3), FOUND)
                .map(|k| k.value),
            Some(4),
            "the world's for anyone"
        );

        knowledge
            .grant(Entity::from_engine(2), RECIPE, Grant::Flag(1))
            .unwrap();
        knowledge.grant_world(FOUND, Grant::Add(2)).unwrap();
        assert_eq!(
            knowledge.grant(Entity::from_engine(2), RECIPE, Grant::Set),
            Err(WorldError::Refused)
        );
        assert_eq!(
            knowledge.grant(Entity::from_engine(2), UNKNOWN, Grant::Set),
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
        assert!(knowledge.completed(Entity::from_engine(2), QUEST));
        assert!(!knowledge.completed(Entity::from_engine(2), OPEN_QUEST));

        let changed = enshwick::abi::headed!(KnowledgeData {
            entity: Entity::from_engine(2),
            id: RECIPE,
            value: 3,
        });
        assert_eq!(loader.raise::<KnowledgeChanged>(&changed), 1);
    }

    #[test]
    fn a_change_naming_both_an_account_and_a_character_or_no_operation_is_refused() {
        let loader = enshwick::test::FakeHost::new().install();
        let recipe = info(RECIPE, KnowledgeInfo::FLAGS, KnowledgeInfo::PLAYER, 0, 0);
        loader.set_knowledge(&[(Some(Entity::from_engine(2)), recipe)], &[]);
        // SAFETY: the fake's table is a static, aligned and never written.
        let api = unsafe { &*loader.api() };
        let change = |account: u64, entity: u32, op: u8| {
            let change = enshwick::abi::headed!(KnowledgeChange {
                account: enshwick::PlayerKey::new(account),
                entity: Entity::from_engine(entity),
                id: RECIPE,
                value: 0,
                kind: KnowledgeInfo::FLAGS,
                op,
                reserved: 0,
            });
            (api.change_knowledge)(loader.handle(), &raw const change)
        };
        assert_eq!(change(7, 2, KnowledgeChange::GRANT), Status::BAD_ARGS);
        assert_eq!(change(0, 2, 2), Status::BAD_ARGS, "an unknown operation");
        assert!(loader.grants().is_empty() && loader.account_changes().is_empty());
        assert_eq!(change(0, 2, KnowledgeChange::GRANT), Status::OK);
        assert_eq!(change(7, 0, KnowledgeChange::REVOKE), Status::OK);
        assert_eq!(loader.grants().len(), 1);
        assert_eq!(loader.account_changes().len(), 1);
    }
}
