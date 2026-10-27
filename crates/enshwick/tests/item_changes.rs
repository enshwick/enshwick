//! A mod changes a field of every vanilla item a selection picks.

#[cfg(test)]
mod tests {
    use enshwick::abi::Status;
    use enshwick::content::categories::ItemCategoryMask;
    use enshwick::content::changes::{Field, Items, ItemsChange, Value};
    use enshwick::test::FakeHost;

    struct Stacks;

    impl enshwick::Mod for Stacks {
        fn manifest() -> &'static enshwick::abi::Manifest {
            enshwick::manifest!(id: "you.stacks", side: BOTH, join: true)
        }

        fn init(load: &mut enshwick::Load<'_>) -> Result<Self, enshwick::Off> {
            let materials = Items::Category(ItemCategoryMask::MATERIALS);
            load.change_items(
                &ItemsChange::new(materials, Field::MaxStackSize, Value::Times(4.0)),
                "stacks",
            )?;
            load.change_items(
                &ItemsChange::new(Items::Containers, Field::Slots, Value::Set(48.0)),
                "chests",
            )?;
            load.change_items(
                &ItemsChange::new(Items::Backpacks, Field::Slots, Value::Times(2.0)),
                "backpacks",
            )?;
            let refused = ItemsChange::new(Items::Stackable, Field::DamageMod, Value::Times(2.0));
            if load.change_items(&refused, "damage").is_ok() {
                return Err(enshwick::Off::new(
                    "a selection changed a field it does not reach",
                ));
            }
            Ok(Self)
        }
    }

    enshwick::declare_mod!(Stacks);

    #[test]
    fn the_selections_reach_the_loader_in_order_with_their_sources() {
        let loader = FakeHost::new().install();
        // SAFETY: `enshwick_mod_init` is the export of this file's `declare_mod!(Stacks)`.
        assert_eq!(unsafe { loader.init(enshwick_mod_init) }, Status::OK);
        let declared: Vec<(Items, Field, String)> = loader
            .item_changes()
            .into_iter()
            .map(|(change, source)| (change.items, change.field, source))
            .collect();
        assert_eq!(
            declared,
            vec![
                (
                    Items::Category(ItemCategoryMask::MATERIALS),
                    Field::MaxStackSize,
                    "stacks".to_owned()
                ),
                (Items::Containers, Field::Slots, "chests".to_owned()),
                (Items::Backpacks, Field::Slots, "backpacks".to_owned()),
            ]
        );
        assert!(loader.changes().is_empty());
    }
}
