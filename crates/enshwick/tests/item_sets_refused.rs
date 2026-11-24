//! A refused item set takes no id and serves no query, so a mod may use the
//! id for something else.

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};

    use enshwick::abi::Status;
    use enshwick::content::journal::{ItemSetEntry, NewItemSet};
    use enshwick::content::knowledge::NewKnowledge;
    use enshwick::content::{Guid, Id};
    use enshwick::knowledge::{KnowledgeType, Scope};
    use enshwick::test::{FakeHost, ITEM_SET_CATEGORY};
    use enshwick::{ItemId, Load, Mod, Off};

    const SHELLS: Id = Id::new("you.refused:shells");

    static TOOK_THE_ID: AtomicBool = AtomicBool::new(false);

    struct Refused;

    impl Mod for Refused {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let refused = load.add_item_set(NewItemSet::new(
                SHELLS,
                ITEM_SET_CATEGORY,
                Guid::named("you.refused:shells.name"),
                vec![ItemSetEntry::new(ItemId::new(0x51))],
            ));
            let why = refused.map(drop).unwrap_err().to_string();
            assert!(why.contains("query database"), "{why}");
            load.add_knowledge(NewKnowledge::new(
                SHELLS,
                Scope::Player,
                KnowledgeType::Bool,
            ))?;
            TOOK_THE_ID.store(true, Ordering::Relaxed);
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Refused,
        r#"
        id = "you.refused"
        side = "both"
        "#
    );

    #[test]
    fn a_set_refused_for_its_queries_leaves_its_id_free() {
        let second = Guid([0x5e; 16]).key_by_hash(enshwick::resources::fnv1a32(
            b"demo::GameKnowledgeQueryResourceDb",
        ));
        let loader = FakeHost::new()
            .progression()
            .world()
            .resource(second, vec![0; 4])
            .install();
        // SAFETY: the three functions are the exports of this file's `declare_mod!(Refused)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        assert!(TOOK_THE_ID.load(Ordering::Relaxed));
    }
}
