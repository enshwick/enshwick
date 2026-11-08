//! Mods take vanilla items, workshops and loot tables out of play and serve
//! a vanilla item with another's data; nothing leaves the game's data.

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use enshwick::abi::{RecipeRuleKind, Status};
    use enshwick::content::changes::{Field, Value as ChangeValue};
    use enshwick::content::items::NewItem;
    use enshwick::content::{EngineFacts, Guid, Id};
    use enshwick::resources::{Registry, Value};
    use enshwick::test::{
        ENTITY_ITEM, FILL_TABLE, FakeHost, Installed, LOOT_ITEM, OTHER_ITEM, RANDOM_LOOT,
        SPARE_ITEM,
    };
    use enshwick::{Load, Mod, Off};

    static REFUSED: Mutex<Vec<String>> = Mutex::new(Vec::new());
    /// The mod's own item, cloned from `OTHER_ITEM`.
    const LANTERN: Id = Id::new("you.removal:lantern");

    struct Removal;

    impl Mod for Removal {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            load.remove_item(LOOT_ITEM)?;
            load.remove_workshop(ENTITY_ITEM)?;
            load.remove_loot_table(FILL_TABLE)?;
            load.rekey_item(OTHER_ITEM, LOOT_ITEM)?;
            // A second re-key of the same item chains on the first.
            load.rekey_item(OTHER_ITEM, SPARE_ITEM)?;
            load.add_item(NewItem::new(LANTERN, OTHER_ITEM))?;
            // `OTHER_ITEM` reads it as served: the lantern's data, until the
            // second re-key above takes `SPARE_ITEM`'s.
            load.rekey_item(LOOT_ITEM, LANTERN.guid())?;
            let mut refused = REFUSED.lock().unwrap();
            for result in [
                load.rekey_item(LOOT_ITEM, ENTITY_ITEM),
                load.remove_loot_table(Guid([7; 16])),
            ] {
                if let Err(off) = result {
                    refused.push(off.to_string());
                }
            }
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Removal,
        r#"
        id = "you.removal"
        side = "both"
        "#
    );

    fn start() -> Installed {
        // The item offsets: a plain clone writes only the id at 0 and the
        // GUID at 4, where the removal preset's items hold them.
        let loader = FakeHost::new().items().removal().install();
        // SAFETY: the three functions are the exports of this file's `declare_mod!(Removal)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        loader
    }

    /// The resource `key` as the mod serves it, built from the game's `base`.
    fn built(loader: &Installed, key: Guid, base: Guid, id: &str, ty: &str) -> Value {
        let host = enshwick::host().unwrap();
        let hash = host.hash(id).unwrap();
        let bytes = host.game_resource(base.key_by_hash(hash)).unwrap();
        let built = loader
            .build_resource(&key.key_by_hash(hash), &bytes)
            .unwrap();
        Registry::game().unwrap().decode(ty, &built).unwrap()
    }

    fn id(value: Option<&Value>) -> Option<u64> {
        match value?.field("value")? {
            Value::Uint(id) => Some(*id),
            _ => None,
        }
    }

    #[test]
    fn a_removed_item_stays_registered_but_no_recipe_or_loot_gives_it() {
        let loader = start();
        let rules = loader.recipe_rules();
        assert!(
            rules.contains(&(RecipeRuleKind::OUTPUT, LOOT_ITEM.to_string())),
            "{rules:?}"
        );
        let counts: Vec<_> = loader
            .changes()
            .into_iter()
            .filter(|(change, _)| change.entry == LOOT_ITEM && change.field == Field::Count)
            .map(|(change, _)| change.value)
            .collect();
        assert_eq!(
            counts,
            [ChangeValue::Set(0.0)],
            "fill tables and starting inventories"
        );
        let random = built(
            &loader,
            RANDOM_LOOT,
            RANDOM_LOOT,
            "lootable_items",
            "demo::LootableItemsResource",
        );
        let Some(Value::List(entries)) = random.field("items") else {
            panic!("{random:?}")
        };
        let left: Vec<_> = entries
            .iter()
            .filter_map(|e| id(e.field("itemId")))
            .collect();
        assert_eq!(left, [u64::from(OTHER_ITEM.fnv1a())]);
    }

    #[test]
    fn a_removed_workshop_makes_nothing_and_is_not_built() {
        let loader = start();
        let rules = loader.recipe_rules();
        for kind in [RecipeRuleKind::WORKSHOP, RecipeRuleKind::OUTPUT] {
            assert!(
                rules.contains(&(kind, ENTITY_ITEM.to_string())),
                "{rules:?}"
            );
        }
    }

    #[test]
    fn a_removed_loot_table_gives_nothing_and_an_unknown_one_is_refused() {
        let loader = start();
        let fill = built(
            &loader,
            FILL_TABLE,
            FILL_TABLE,
            "loot_fill_table",
            "demo::ecs::DefaultInventoryResource",
        );
        let root = fill.field("rootGroup").unwrap();
        assert_eq!(root.field("stacks"), Some(&Value::List(vec![])));
        assert_eq!(
            root.field("lootLabels")
                .and_then(|labels| labels.field("labels")),
            Some(&Value::List(vec![])),
            "no loot by label"
        );
        let refused = REFUSED.lock().unwrap().clone();
        assert!(
            refused.iter().any(|r| r.contains("not in the game")),
            "{refused:?}"
        );
    }

    #[test]
    fn a_rekeyed_item_keeps_its_id_guid_and_stack_and_chains_and_an_entity_mismatch_is_refused() {
        let loader = start();
        let item = built(
            &loader,
            OTHER_ITEM,
            OTHER_ITEM,
            "item_info",
            "demo::ItemInfo",
        );
        assert_eq!(
            id(item.field("itemId")),
            Some(u64::from(OTHER_ITEM.fnv1a()))
        );
        assert_eq!(item.field("objectId"), Some(&Value::Guid(OTHER_ITEM)));
        assert_eq!(
            item.field("maxStackSize"),
            Some(&Value::Uint(50)),
            "the old item's 50 over 20 and 30"
        );
        assert_eq!(
            item.field("category"),
            Some(&Value::Uint(u64::from(SPARE_ITEM.0[15]))),
            "the last re-key's data"
        );
        let refused = REFUSED.lock().unwrap().clone();
        assert!(
            refused.iter().any(|r| r.contains("entity of its own")),
            "{refused:?}"
        );
    }

    #[test]
    fn an_item_rekeyed_to_the_mod_s_own_item_takes_that_item_s_data_under_its_own_id() {
        let loader = start();
        let item = built(&loader, LOOT_ITEM, LOOT_ITEM, "item_info", "demo::ItemInfo");
        assert_eq!(id(item.field("itemId")), Some(u64::from(LOOT_ITEM.fnv1a())));
        assert_eq!(item.field("objectId"), Some(&Value::Guid(LOOT_ITEM)));
        assert_eq!(
            item.field("category"),
            Some(&Value::Uint(u64::from(OTHER_ITEM.0[15]))),
            "the data of the lantern's base item"
        );
        assert_eq!(
            item.field("maxStackSize"),
            Some(&Value::Uint(50)),
            "the base item's 50 over the old item's 20"
        );
    }

    #[test]
    fn a_pack_cannot_rekey_to_an_item_another_pack_added() {
        use enshwick::abi::ModHandle;

        let loader = FakeHost::new().items().removal().install();
        // SAFETY: the fake host's table lives for the process.
        unsafe { enshwick::__private::install_host(loader.api(), ModHandle(9)) };
        let init = |me, call: fn(&mut Load<'_>) -> Result<(), Off>| {
            // SAFETY: the fake host's table lives for the process.
            unsafe { enshwick::__private::init_pack(loader.api(), ModHandle(me), call) }
        };
        let added = init(1, |load| load.add_item(NewItem::new(LANTERN, OTHER_ITEM)));
        assert_eq!(added, Status::OK, "{:?}", loader.logs());
        let rekeyed = init(2, |load| load.rekey_item(SPARE_ITEM, LANTERN.guid()));
        assert_ne!(rekeyed, Status::OK, "the lantern is the first pack's");
        let own = init(1, |load| load.rekey_item(SPARE_ITEM, LANTERN.guid()));
        assert_eq!(own, Status::OK, "{:?}", loader.logs());
    }
}
