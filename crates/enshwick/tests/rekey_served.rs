//! A re-keyed item takes its new item as the game reads it: other mods'
//! changes of that item reach it, and two items re-keyed to each other swap.

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};

    use enshwick::abi::{ModHandle, Status};
    use enshwick::content::items::NewItem;
    use enshwick::content::{Guid, Id, types};
    use enshwick::resources::{Registry, Value};
    use enshwick::test::{ENTITY_ITEM, FakeHost, Installed, LOOT_ITEM, OTHER_ITEM, SPARE_ITEM};
    use enshwick::{Load, Mod, Off};

    /// The mod's own item, cloned from `OTHER_ITEM`.
    const LANTERN: Id = Id::new("you.rekey:lantern");
    /// The category another mod gives the lantern.
    const CHANGED_CATEGORY: u8 = 0x77;
    /// Where the fake's items hold their category byte.
    const CATEGORY_AT: usize = 22;

    static REFUSED_AT_INIT: AtomicBool = AtomicBool::new(false);

    struct Rekey;

    impl Mod for Rekey {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let other = OTHER_ITEM.key(types::ITEM_INFO)?;
            REFUSED_AT_INIT.store(
                load.host().served_resource(other).is_err(),
                Ordering::Relaxed,
            );
            load.add_item(NewItem::new(LANTERN, OTHER_ITEM))?;
            load.rekey_item(SPARE_ITEM, LANTERN.guid())?;
            load.rekey_item(LOOT_ITEM, OTHER_ITEM)?;
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Rekey,
        r#"
        id = "you.rekey"
        side = "both"
        "#
    );

    /// Two other mods first: one changes the lantern's category, one
    /// re-keys `OTHER_ITEM` back to `LOOT_ITEM`; then this file's mod.
    fn start() -> Installed {
        let loader = FakeHost::new().items().removal().install();
        let pack = |me, call: fn(&mut Load<'_>) -> Result<(), Off>| {
            // SAFETY: the fake host's table lives for the process.
            let status =
                unsafe { enshwick::__private::init_pack(loader.api(), ModHandle(me), call) };
            assert_eq!(status, Status::OK, "{:?}", loader.logs());
        };
        pack(7, |load| {
            let key = LANTERN.guid().key(types::ITEM_INFO)?;
            load.serve(key, key, |item| {
                let mut item = item.to_vec();
                *item.get_mut(CATEGORY_AT)? = CHANGED_CATEGORY;
                Some(item)
            })
        });
        pack(8, |load| load.rekey_item(OTHER_ITEM, LOOT_ITEM));
        // SAFETY: the three functions are the exports of this file's `declare_mod!(Rekey)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        loader
    }

    /// The vanilla item `item` as the mods serve it.
    fn built(loader: &Installed, item: Guid) -> Value {
        let host = enshwick::host().unwrap();
        let key = item.key(types::ITEM_INFO).unwrap();
        let built = loader
            .build_resource(&key, &host.game_resource(key).unwrap())
            .unwrap();
        Registry::game()
            .unwrap()
            .decode("demo::ItemInfo", &built)
            .unwrap()
    }

    fn category(item: Guid) -> Value {
        Value::Uint(u64::from(item.0[15]))
    }

    #[test]
    fn an_item_rekeyed_to_the_mod_s_own_item_takes_another_mod_s_change_of_it() {
        let loader = start();
        let item = built(&loader, SPARE_ITEM);
        assert_eq!(item.field("objectId"), Some(&Value::Guid(SPARE_ITEM)));
        assert_eq!(
            item.field("category"),
            Some(&Value::Uint(u64::from(CHANGED_CATEGORY))),
            "the lantern as served, not as cloned from its base"
        );
    }

    #[test]
    fn two_items_rekeyed_to_each_other_by_two_mods_swap() {
        let loader = start();
        let loot = built(&loader, LOOT_ITEM);
        assert_eq!(loot.field("objectId"), Some(&Value::Guid(LOOT_ITEM)));
        assert_eq!(loot.field("category"), Some(&category(OTHER_ITEM)));
        let other = built(&loader, OTHER_ITEM);
        assert_eq!(other.field("objectId"), Some(&Value::Guid(OTHER_ITEM)));
        assert_eq!(other.field("category"), Some(&category(LOOT_ITEM)));
        let logs = loader.logs();
        assert!(
            !logs.iter().any(|(_, line)| line.contains("refused")),
            "a swap is no failure: {logs:?}"
        );
    }

    #[test]
    fn a_resource_reads_as_served_only_once_every_mod_s_init_is_done() {
        let _loader = start();
        assert!(
            REFUSED_AT_INIT.load(Ordering::Relaxed),
            "not every patch is in"
        );
        let host = enshwick::host().unwrap();
        let lantern = host
            .served_resource(LANTERN.guid().key(types::ITEM_INFO).unwrap())
            .unwrap();
        let value = Registry::game()
            .unwrap()
            .decode("demo::ItemInfo", &lantern)
            .unwrap();
        assert_eq!(
            value.field("category"),
            Some(&Value::Uint(u64::from(CHANGED_CATEGORY)))
        );
        let unserved = ENTITY_ITEM.key(types::ITEM_INFO).unwrap();
        assert_eq!(
            host.served_resource(unserved).ok(),
            host.game_resource(unserved),
            "a key no mod serves reads as archived"
        );
    }
}
