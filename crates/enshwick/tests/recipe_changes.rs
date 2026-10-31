//! A mod changes a field of every vanilla recipe a selection picks.

#[cfg(test)]
mod tests {
    use enshwick::abi::Status;
    use enshwick::content::Guid;
    use enshwick::content::changes::{Field, Recipes, RecipesChange, Value};
    use enshwick::content::recipes::RecipeKind;
    use enshwick::content::workshops::WorkshopId;
    use enshwick::test::FakeHost;

    const KILN: WorkshopId = WorkshopId::new(Guid([7; 16]));

    struct Cheap;

    impl enshwick::Mod for Cheap {
        fn init(load: &mut enshwick::Load<'_>) -> Result<Self, enshwick::Off> {
            load.change_recipes(
                &RecipesChange::new(
                    Recipes::Kind(RecipeKind::Roof),
                    Field::Cost,
                    Value::Times(0.5),
                ),
                "roofs",
            )?;
            load.change_recipes(
                &RecipesChange::new(
                    Recipes::Workshop(KILN),
                    Field::CraftingDuration,
                    Value::Times(0.5),
                ),
                "kiln",
            )?;
            let refused = RecipesChange::new(
                Recipes::Workshop(KILN),
                Field::MaxStackSize,
                Value::Set(2.0),
            );
            if load.change_recipes(&refused, "stacks").is_ok() {
                return Err(enshwick::Off::new(
                    "a recipe selection changed an item field",
                ));
            }
            let ingredient =
                RecipesChange::new(Recipes::Workshop(KILN), Field::Ingredient, Value::Set(2.0));
            if load.change_recipes(&ingredient, "ingredient").is_ok() {
                return Err(enshwick::Off::new(
                    "a recipe selection changed one ingredient",
                ));
            }
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Cheap,
        r#"
        id = "you.cheap"
        side = "both"
        join = true
        "#
    );

    #[test]
    fn recipe_selections_reach_the_loader_in_order_with_their_sources() {
        let loader = FakeHost::new().install();
        // SAFETY: `enshwick_mod_init` is the export of this file's `declare_mod!(Cheap)`.
        assert_eq!(unsafe { loader.init(enshwick_mod_init) }, Status::OK);
        let declared: Vec<(Recipes, Field, String)> = loader
            .recipe_changes()
            .into_iter()
            .map(|(change, source)| (change.recipes, change.field, source))
            .collect();
        assert_eq!(
            declared,
            vec![
                (
                    Recipes::Kind(RecipeKind::Roof),
                    Field::Cost,
                    "roofs".to_owned()
                ),
                (
                    Recipes::Workshop(KILN),
                    Field::CraftingDuration,
                    "kiln".to_owned()
                ),
            ]
        );
        assert!(loader.item_changes().is_empty());
    }
}
