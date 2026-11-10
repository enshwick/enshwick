//! Knowledge taken back, knowledge changed for an account, and the filter on
//! what a client asks for: each reaches the loader as the mod meant it.

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use enshwick::abi::{Header, KnowledgeInfo, KnowledgeInput, Status};
    use enshwick::knowledge::{Grant, Knowledge, KnowledgeId, Refusal, Revoke};
    use enshwick::{Entity, Load, Mod, Off, PlayerKey};

    static KNOWLEDGE: OnceLock<Knowledge> = OnceLock::new();

    const RECIPE: KnowledgeId = KnowledgeId::new(0x1d);
    const TUTORIAL_COUNTER: KnowledgeId = KnowledgeId::new(0x51);
    const PLACE: KnowledgeId = KnowledgeId::new(0x77);
    const ACCOUNT: PlayerKey = PlayerKey(0x0110_0001_0000_0042);

    struct Teacher;

    impl Mod for Teacher {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let _first = KNOWLEDGE.set(load.knowledge());
            load.on_knowledge_input(|request| {
                request.allowed
                    || (request.id == TUTORIAL_COUNTER && request.refusal == Some(Refusal::Counter))
            })?;
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Teacher,
        r#"
        id = "you.teacher"
        side = "server"
        "#
    );

    fn info(id: KnowledgeId, kind: u8, scope: u8) -> KnowledgeInfo {
        KnowledgeInfo {
            header: Header::of::<KnowledgeInfo>(),
            id,
            kind,
            scope,
            reserved: 0,
            value: 0,
            flags: 0,
        }
    }

    /// The probe's ids as the game knows them, so every call names an id of
    /// the kind and scope the loader takes.
    fn started() -> (enshwick::test::Installed, &'static Knowledge) {
        let loader = enshwick::test::FakeHost::new().install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Teacher)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.faults());
        let player = Some(Entity(2));
        loader.set_knowledge(
            &[
                (
                    player,
                    info(RECIPE, KnowledgeInfo::FLAGS, KnowledgeInfo::PLAYER),
                ),
                (
                    player,
                    info(
                        TUTORIAL_COUNTER,
                        KnowledgeInfo::COUNTER,
                        KnowledgeInfo::PLAYER,
                    ),
                ),
                (None, info(PLACE, KnowledgeInfo::BOOL, KnowledgeInfo::WORLD)),
            ],
            &[],
        );
        (loader, KNOWLEDGE.get().unwrap())
    }

    #[test]
    fn a_revoke_reaches_the_loader_with_its_kind_and_value() {
        let (loader, knowledge) = started();
        knowledge
            .revoke(Entity(2), RECIPE, Revoke::Unflag(1))
            .unwrap();
        knowledge
            .revoke(Entity(2), TUTORIAL_COUNTER, Revoke::Sub(3))
            .unwrap();
        knowledge.revoke_world(PLACE, Revoke::Clear).unwrap();
        assert_eq!(
            loader.revokes(),
            [
                (Some(Entity(2)), RECIPE, u32::from(KnowledgeInfo::FLAGS), 1),
                (
                    Some(Entity(2)),
                    TUTORIAL_COUNTER,
                    u32::from(KnowledgeInfo::COUNTER),
                    3
                ),
                (None, PLACE, u32::from(KnowledgeInfo::BOOL), 0),
            ]
        );
        assert!(loader.grants().is_empty());
    }

    #[test]
    fn an_account_change_reaches_the_loader_as_a_grant_or_a_revoke() {
        let (loader, knowledge) = started();
        knowledge
            .grant_account(ACCOUNT, RECIPE, Grant::Flag(0))
            .unwrap();
        knowledge
            .revoke_account(ACCOUNT, RECIPE, Revoke::Unflag(1))
            .unwrap();
        assert_eq!(
            loader.account_changes(),
            [
                (ACCOUNT, RECIPE, u32::from(KnowledgeInfo::FLAGS), 0, false),
                (ACCOUNT, RECIPE, u32::from(KnowledgeInfo::FLAGS), 1, true),
            ]
        );
    }

    fn input(id: KnowledgeId, kind: u8, refusal: u32, allowed: bool) -> KnowledgeInput {
        KnowledgeInput {
            header: Header::of::<KnowledgeInput>(),
            player: Entity(2),
            id,
            kind,
            scope: KnowledgeInfo::PLAYER,
            reserved: 0,
            value: 1,
            refusal,
            allowed: u32::from(allowed),
        }
    }

    #[test]
    fn an_input_filter_keeps_the_guard_s_verdict_or_allows_what_it_names() {
        let (loader, _knowledge) = started();
        assert!(loader.knowledge_input(input(
            RECIPE,
            KnowledgeInfo::FLAGS,
            KnowledgeInput::ALLOWED,
            true
        )));
        assert!(!loader.knowledge_input(input(
            RECIPE,
            KnowledgeInfo::BOOL,
            KnowledgeInput::KIND,
            false
        )));
        assert!(
            loader.knowledge_input(input(
                TUTORIAL_COUNTER,
                KnowledgeInfo::COUNTER,
                KnowledgeInput::COUNTER,
                false
            )),
            "the filter allows the tutorial's counter"
        );
    }
}
