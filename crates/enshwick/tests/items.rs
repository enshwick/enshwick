//! Item filters, as the loader hands them players' actions with items.

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use enshwick::abi::{Header, ItemAction, Status};
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
        ItemAction {
            header: Header::of::<ItemAction>(),
            kind,
            player: Entity(4096),
            item,
            count: 3,
            inventory: Entity(inventory),
            slot: 2,
        }
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
        assert!(
            loader.item_action(&action(ItemAction::EQUIP, tonic(), 4100)),
            "a panic in a filter allows the action"
        );
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
        let late = (api.item_filter)(loader.handle(), refuse_all, core::ptr::null_mut());
        assert_eq!(late, Status::TOO_LATE, "a filter after init is refused");
    }

    unsafe extern "system" fn refuse_all(_: *const ItemAction, _: *mut core::ffi::c_void) -> u8 {
        0
    }
}
