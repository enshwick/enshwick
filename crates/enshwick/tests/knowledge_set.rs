//! Knowledge set to an exact value: each set reaches the loader with its
//! notice, and what the loader refuses comes back refused.

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use enshwick::abi::{KnowledgeChange, KnowledgeInfo, Status};
    use enshwick::knowledge::{Knowledge, KnowledgeId, KnowledgeNotice};
    use enshwick::{Entity, Load, Mod, Off, WorldError};

    static KNOWLEDGE: OnceLock<Knowledge> = OnceLock::new();

    const RECIPE: KnowledgeId = KnowledgeId::new(0x1d);
    const LISTED: KnowledgeId = KnowledgeId::new(0x2d);
    const VISITS: KnowledgeId = KnowledgeId::new(0x51);
    const PLACE: KnowledgeId = KnowledgeId::new(0x77);

    struct Setter;

    impl Mod for Setter {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let _first = KNOWLEDGE.set(load.knowledge());
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Setter,
        r#"
        id = "you.setter"
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

    fn started() -> (enshwick::test::Installed, &'static Knowledge) {
        let loader = enshwick::test::FakeHost::new().install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Setter)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.faults());
        let player = Some(Entity::from_engine(2));
        loader.set_knowledge(
            &[
                (
                    player,
                    info(RECIPE, KnowledgeInfo::FLAGS, KnowledgeInfo::PLAYER, 0, 0),
                ),
                (
                    player,
                    info(
                        LISTED,
                        KnowledgeInfo::FLAGS,
                        KnowledgeInfo::PLAYER,
                        0b001,
                        0b101,
                    ),
                ),
                (
                    player,
                    info(VISITS, KnowledgeInfo::COUNTER, KnowledgeInfo::PLAYER, 4, 0),
                ),
                (
                    None,
                    info(PLACE, KnowledgeInfo::BOOL, KnowledgeInfo::WORLD, 0, 0),
                ),
            ],
            &[],
        );
        (loader, KNOWLEDGE.get().unwrap())
    }

    #[test]
    fn a_set_reaches_the_loader_with_its_value_and_notice() {
        let (loader, knowledge) = started();
        knowledge
            .set(Entity::from_engine(2), RECIPE, 0b11, KnowledgeNotice::Quiet)
            .unwrap();
        knowledge
            .set(Entity::from_engine(2), VISITS, 1, KnowledgeNotice::Shown)
            .unwrap();
        knowledge
            .set_world(PLACE, 1, KnowledgeNotice::Shown)
            .unwrap();
        knowledge
            .set(Entity::from_engine(2), PLACE, 0, KnowledgeNotice::Quiet)
            .unwrap();
        assert_eq!(
            loader.knowledge_sets(),
            [
                (
                    Some(Entity::from_engine(2)),
                    RECIPE,
                    0b11,
                    KnowledgeChange::QUIET
                ),
                (
                    Some(Entity::from_engine(2)),
                    VISITS,
                    1,
                    KnowledgeChange::SHOWN
                ),
                (None, PLACE, 1, KnowledgeChange::SHOWN),
                (None, PLACE, 0, KnowledgeChange::QUIET),
            ],
            "a world id set through a character is the world's"
        );
        assert!(loader.grants().is_empty() && loader.revokes().is_empty());
    }

    #[test]
    fn a_set_of_an_unknown_id_a_bool_above_one_or_a_player_s_id_on_the_world_is_refused() {
        let (loader, knowledge) = started();
        let refused = [
            (
                knowledge.set(
                    Entity::from_engine(2),
                    KnowledgeId::new(0x99),
                    1,
                    KnowledgeNotice::Quiet,
                ),
                WorldError::BadArgs,
            ),
            (
                knowledge.set_world(PLACE, 2, KnowledgeNotice::Quiet),
                WorldError::BadArgs,
            ),
            (
                knowledge.set_world(RECIPE, 1, KnowledgeNotice::Quiet),
                WorldError::Refused,
            ),
        ];
        for (call, (answer, error)) in refused.into_iter().enumerate() {
            assert_eq!(answer, Err(error), "call {call}");
        }
        assert!(loader.knowledge_sets().is_empty());
    }

    #[test]
    fn a_shown_set_refuses_a_gained_bit_the_game_does_not_expect_and_a_quiet_one_writes_it() {
        let (loader, knowledge) = started();
        assert_eq!(
            knowledge.set(
                Entity::from_engine(2),
                LISTED,
                0b011,
                KnowledgeNotice::Shown
            ),
            Err(WorldError::BadArgs)
        );
        knowledge
            .set(
                Entity::from_engine(2),
                LISTED,
                0b101,
                KnowledgeNotice::Shown,
            )
            .unwrap();
        knowledge
            .set(
                Entity::from_engine(2),
                LISTED,
                0b011,
                KnowledgeNotice::Quiet,
            )
            .unwrap();
        assert_eq!(loader.knowledge_sets().len(), 2);
    }
}
