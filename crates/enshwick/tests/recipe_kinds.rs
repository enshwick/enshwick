//! A mod reads the kind of item each recipe makes, as the loader answers
//! it, and changes recipes by kind; a recipe whose item the game has not
//! loaded has no kind yet.

#[cfg(test)]
mod tests {
    use enshwick::abi::Status;
    use enshwick::content::changes::{Field, Recipes, RecipesChange, Value};
    use enshwick::content::recipes::{Derived, RECIPE_REGISTRY, RecipeKind, RecipeLayout};
    use enshwick::content::{Blob, EngineFacts, Guid, types};
    use enshwick::rates::Rate;
    use enshwick::test::FakeHost;
    use enshwick::{Load, Mod, Off};

    const PLANK: Guid = Guid::parse("0b6c5a52-1df4-4a3b-bd0e-6c1f7e2a9c44");
    const ROOF: Guid = Guid::parse("1d2c3b4a-5e6f-4a1b-8c2d-3e4f5a6b7c8d");
    const VASE: Guid = Guid::parse("3f4e5d6c-7a8b-4c3d-8e4f-5a6b7c8d9e0f");
    const CHAIR: Guid = Guid::parse("4a5b6c7d-8e9f-4a0b-9c1d-2e3f4a5b6c7d");
    const CHEST: Guid = Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d");
    const MOSS: Guid = Guid::parse("2e3d4c5b-6f7a-4b2c-9d3e-4f5a6b7c8d9e");

    /// How many each derived copy makes, by the kind of its original: a
    /// distinct count per kind, so the built registry tells which kind each
    /// recipe read.
    fn copies(kind: RecipeKind) -> u32 {
        match kind {
            RecipeKind::BuildingBlock => 2,
            RecipeKind::Roof => 3,
            RecipeKind::Decorative => 4,
            RecipeKind::Other => 5,
            RecipeKind::ComfortFurniture => 6,
            _ => 7,
        }
    }

    struct Kinds;

    impl Mod for Kinds {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            load.derive_recipes("you.kinds", |recipe| {
                recipe.kind().map(|kind| Derived {
                    output: copies(kind),
                    cost: 1,
                    workshop: None,
                })
            })?;
            load.change_recipes(
                &RecipesChange::new(
                    Recipes::Kind(RecipeKind::Decorative),
                    Field::Output,
                    Value::Times(2.0),
                ),
                "decor",
            )?;
            load.rate(Rate::RecipeOutput(RecipeKind::Decorative))?
                .set(1.5)?;
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Kinds,
        r#"
        id = "you.kinds"
        side = "both"
        join = true
        "#
    );

    /// A registry of one recipe per item in `outputs`, each making one.
    fn registry(outputs: &[Guid]) -> Vec<u8> {
        let layout = layout();
        let mut blob = Blob::new(&[0u8; 16]);
        let first = blob.append(&vec![0; layout.recipe * outputs.len()], 8);
        for (i, output) in outputs.iter().enumerate() {
            let at = first + i * layout.recipe;
            let guid = [u8::try_from(i + 1).unwrap(); 16];
            assert!(blob.write(at + layout.guid, &guid));
            let mut stack = vec![0u8; layout.stack_size];
            stack[..16].copy_from_slice(&output.0);
            let count = layout.stack_count;
            stack[count..count + 4].copy_from_slice(&1u32.to_le_bytes());
            let stack_at = blob.append(&stack, 8);
            assert!(blob.set_array(at + layout.output, stack_at, 1));
        }
        let count = u32::try_from(outputs.len()).unwrap();
        assert!(blob.set_array(layout.recipes, first, count));
        blob.into_bytes()
    }

    /// What each recipe of `bytes` makes and how many.
    fn made(bytes: &[u8]) -> Vec<(Guid, u32)> {
        let layout = layout();
        let blob = Blob::new(bytes);
        let (first, count) = blob.array(layout.recipes).unwrap();
        (0..count as usize)
            .map(|i| {
                let (stack, _) = blob
                    .array(first + i * layout.recipe + layout.output)
                    .unwrap();
                let mut guid = [0u8; 16];
                guid.copy_from_slice(&blob.bytes()[stack..stack + 16]);
                let count = blob.read_u32(stack + layout.stack_count).unwrap();
                (Guid(guid), count)
            })
            .collect()
    }

    fn host() -> FakeHost {
        FakeHost::new().recipes()
    }

    /// The recipe layout `host` gives.
    fn layout() -> RecipeLayout {
        RecipeLayout::resolve(&host()).unwrap()
    }

    /// The hash the recipe registry is served under.
    fn recipes() -> u32 {
        host().hash(types::RECIPE_REGISTRY_RESOURCE.id()).unwrap()
    }

    #[test]
    fn a_recipe_reads_the_kind_of_its_item_once_it_is_known() {
        let loader = host()
            .recipe_kind(PLANK, RecipeKind::BuildingBlock)
            .recipe_kind(ROOF, RecipeKind::Roof)
            .recipe_kind(VASE, RecipeKind::Decorative)
            .recipe_kind(CHAIR, RecipeKind::ComfortFurniture)
            .recipe_kind(MOSS, RecipeKind::Other)
            .install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Kinds)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let key = RECIPE_REGISTRY.key_by_hash(recipes());
        let built = loader
            .build_resource(&key, &registry(&[PLANK, ROOF, VASE, CHAIR, MOSS, CHEST]))
            .unwrap();
        let copies: Vec<(Guid, u32)> = made(&built).into_iter().skip(6).collect();
        assert_eq!(
            copies,
            [(PLANK, 2), (ROOF, 3), (VASE, 4), (CHAIR, 6), (MOSS, 5)],
            "the chest's item has no kind yet, so it has no copy"
        );
    }

    #[test]
    fn decorative_recipes_take_data_changes_and_rates_by_their_kind() {
        let loader = host().install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Kinds)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let declared: Vec<(Recipes, Field)> = loader
            .recipe_changes()
            .into_iter()
            .map(|(change, _)| (change.recipes, change.field))
            .collect();
        assert_eq!(
            declared,
            [(Recipes::Kind(RecipeKind::Decorative), Field::Output)]
        );
        assert_eq!(
            loader.rate(Rate::RecipeOutput(RecipeKind::Decorative)),
            Some(1.5)
        );
    }
}
