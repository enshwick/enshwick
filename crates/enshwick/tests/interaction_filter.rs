//! Interaction filters: the interaction the game asks about reaches the
//! mod's filter, and a refusal comes back as one.

#[cfg(test)]
mod tests {
    use enshwick::abi::{Header, Interaction, Status};
    use enshwick::interactions::Kind;
    use enshwick::{Entity, ItemId, Load, Mod, Off};

    const LOCKED_CHEST: ItemId = ItemId::new(0x5e01);

    struct Locks;

    impl Mod for Locks {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            load.on_interaction(|i| {
                !(i.item == Some(LOCKED_CHEST) && matches!(i.kind, Kind::Open | Kind::Move))
            })?;
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Locks,
        r#"
        id = "you.locks"
        side = "server"
        "#
    );

    fn call(kind: u32, item: ItemId, known: u8, player: Entity) -> Interaction {
        Interaction {
            header: Header::of::<Interaction>(),
            kind,
            player,
            target: Entity(4096),
            item,
            known,
            reserved: [0; 3],
            position: [1.0, 2.0, 3.0],
        }
    }

    #[test]
    fn a_filter_refuses_the_kinds_it_names_on_the_item_it_names() {
        let loader = enshwick::test::FakeHost::new().install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Locks)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.faults());
        let player = Entity(2);

        assert!(!loader.interaction(&call(Interaction::OPEN, LOCKED_CHEST, 1, player)));
        assert!(!loader.interaction(&call(Interaction::MOVE, LOCKED_CHEST, 1, player)));
        assert!(loader.interaction(&call(Interaction::SORT, LOCKED_CHEST, 1, player)));
        assert!(loader.interaction(&call(Interaction::OPEN, ItemId::new(7), 1, player)));
        assert!(
            loader.interaction(&call(Interaction::OPEN, LOCKED_CHEST, 0, player)),
            "an item the platform could not read is no item"
        );
        assert!(
            loader.interaction(&call(Interaction::OPEN, LOCKED_CHEST, 1, Entity(0))),
            "an interaction without a character is allowed unasked"
        );
    }
}
