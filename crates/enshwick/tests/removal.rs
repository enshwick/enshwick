//! Mods take vanilla items, workshops and loot tables out of play and serve
//! a vanilla item with another's data; nothing leaves the game's data.

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use enshwick::abi::{RecipeRuleKind, Status};
    use enshwick::content::changes::{Field, Value as ChangeValue};
    use enshwick::content::{EngineFacts, Guid};
    use enshwick::resources::{Registry, Value};
    use enshwick::test::{
        ENTITY_ITEM, FILL_TABLE, FakeHost, Installed, LOOT_ITEM, OTHER_ITEM, RANDOM_LOOT,
    };
    use enshwick::{Load, Mod, Off};

    static REFUSED: Mutex<Vec<String>> = Mutex::new(Vec::new());

    struct Removal;

    impl Mod for Removal {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            load.remove_item(LOOT_ITEM)?;
            load.remove_workshop(ENTITY_ITEM)?;
            load.remove_loot_table(FILL_TABLE)?;
            load.rekey_item(OTHER_ITEM, LOOT_ITEM)?;
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
        let loader = FakeHost::new().removal().install();
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
        let stacks = fill
            .field("rootGroup")
            .and_then(|root| root.field("stacks"));
        assert_eq!(stacks, Some(&Value::List(vec![])));
        let refused = REFUSED.lock().unwrap().clone();
        assert!(
            refused.iter().any(|r| r.contains("not in the game")),
            "{refused:?}"
        );
    }

    #[test]
    fn a_rekeyed_item_keeps_its_id_and_stack_and_one_with_another_entity_kind_is_refused() {
        let loader = start();
        let item = built(
            &loader,
            OTHER_ITEM,
            LOOT_ITEM,
            "item_info",
            "demo::ItemInfo",
        );
        assert_eq!(
            id(item.field("itemId")),
            Some(u64::from(OTHER_ITEM.fnv1a()))
        );
        assert_eq!(
            item.field("maxStackSize"),
            Some(&Value::Uint(50)),
            "the old item's 50 over 20"
        );
        let refused = REFUSED.lock().unwrap().clone();
        assert!(
            refused.iter().any(|r| r.contains("entity of its own")),
            "{refused:?}"
        );
    }
}
