//! Item filters, as the loader hands them players' actions with items.

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use enshwick::abi::{ItemAction, Status};
    use enshwick::content::{Id, id};
    use enshwick::items::Kind;
    use enshwick::{Entity, ItemId, Load, Mod, Off};

    const TONIC: Id = id!("tonic");
    static DRUNK: AtomicU32 = AtomicU32::new(0);
    static PICKED: AtomicU32 = AtomicU32::new(0);

    fn tonic() -> ItemId {
        ItemId::from_guid(TONIC.guid())
    }

    struct Tonics;

    impl Mod for Tonics {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            load.on_item(tonic(), Kind::Consume, |used| {
                DRUNK.fetch_add(used.count, Ordering::Relaxed);
                used.slot.is_some()
            })?;
            load.on_item(tonic(), Kind::Trash, |_| false)?;
            load.on_item(tonic(), Kind::Equip, |_| panic!("a tonic is never worn"))?;
            load.on_item(tonic(), Kind::Pickup, |picked| {
                PICKED.fetch_add(picked.count, Ordering::Relaxed);
                false
            })?;
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Tonics,
        r#"
        id = "you.tonics"
        side = "server"
        "#
    );

    fn action(kind: u32, item: ItemId, inventory: u32) -> ItemAction {
        enshwick::abi::headed!(ItemAction {
            kind,
            character: Entity::from_engine(4096),
            item,
            count: 3,
            inventory: Entity::from_engine(inventory),
            slot: 2,
        })
    }

    #[test]
    fn each_kind_reaches_only_its_handler_and_a_refusal_holds_where_it_can() {
        let loader = enshwick::test::FakeHost::new().install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Tonics)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);

        assert!(loader.item_action(&action(ItemAction::CONSUME, tonic(), 4100)));
        assert!(
            !loader.item_action(&action(ItemAction::CONSUME, tonic(), 0)),
            "a use without a slot is refused by the handler"
        );
        assert!(!loader.item_action(&action(ItemAction::TRASH, tonic(), 4100)));
        assert!(loader.item_action(&action(ItemAction::UNEQUIP, tonic(), 4100)));
        assert!(
            loader.item_action(&action(ItemAction::PICKUP, tonic(), 0)),
            "a pickup cannot be refused"
        );
        assert_eq!(DRUNK.load(Ordering::Relaxed), 6);
        assert_eq!(PICKED.load(Ordering::Relaxed), 3);

        let other = ItemId::named("you.lanterns:lantern");
        assert!(loader.item_action(&action(ItemAction::TRASH, other, 4100)));
        assert!(loader.item_action(&action(ItemAction::CONSUME, other, 4100)));
        assert_eq!(
            DRUNK.load(Ordering::Relaxed),
            6,
            "another mod's item stays untouched"
        );

        // SAFETY: the fake's table lives for the process.
        let api = unsafe { &*loader.api() };
        let late = (api.item_filter)(loader.handle(), Some(refuse_all), core::ptr::null_mut());
        assert_eq!(late, Status::TOO_LATE, "a filter after init is refused");

        assert!(
            loader.item_action(&action(ItemAction::EQUIP, tonic(), 4100)),
            "a panic in a filter allows the action"
        );
        assert!(loader.is_off(), "the panic turned the mod off");
        assert!(
            loader.item_action(&action(ItemAction::TRASH, tonic(), 4100)),
            "an off mod's filter refuses nothing"
        );
    }

    fn moved(item: ItemId, action: u8) -> enshwick::abi::Interaction {
        let mut call = enshwick::abi::Interaction::new(
            enshwick::abi::Interaction::MOVE,
            Entity::from_engine(4096),
            Entity::from_engine(4100),
        );
        call.moving = 1;
        call.moved = item;
        call.stack = 3;
        call.action = action;
        call.from_slot = 4100 | 2 << 32;
        call
    }

    #[test]
    fn a_stack_the_backpack_deletes_reaches_the_trash_handler() {
        use enshwick::abi::Interaction;
        let loader = enshwick::test::FakeHost::new().install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Tonics)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);

        assert!(!loader.interaction(&moved(tonic(), Interaction::ACTION_DESTROY)));
        assert!(
            loader.interaction(&moved(tonic(), Interaction::ACTION_ALL)),
            "a plain move is not a trash"
        );
        assert!(
            loader.interaction(&moved(
                ItemId::named("you.lanterns:lantern"),
                Interaction::ACTION_DESTROY
            )),
            "another item's delete goes ahead"
        );
    }

    unsafe extern "system" fn refuse_all(_: *const ItemAction, _: *mut core::ffi::c_void) -> u8 {
        0
    }
}
