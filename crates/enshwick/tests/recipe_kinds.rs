//! A mod reads the kind of item each recipe makes, as the loader answers
//! it, and changes recipes by kind; a recipe whose item the game has not
//! loaded has no kind yet.

#[cfg(test)]
mod tests {
    use enshwick::abi::Status;
    use enshwick::content::changes::{Field, Recipes, RecipesChange, Value};
    use enshwick::content::recipes::{Derived, RECIPE_REGISTRY, RecipeKind};
    use enshwick::content::{Blob, Guid, types};
    use enshwick::rates::Rate;
    use enshwick::test::FakeHost;
    use enshwick::{Load, Mod, Off};

    const PLANK: Guid = Guid::parse("0b6c5a52-1df4-4a3b-bd0e-6c1f7e2a9c44");
    const ROOF: Guid = Guid::parse("1d2c3b4a-5e6f-4a1b-8c2d-3e4f5a6b7c8d");
    const VASE: Guid = Guid::parse("3f4e5d6c-7a8b-4c3d-8e4f-5a6b7c8d9e0f");
    const CHAIR: Guid = Guid::parse("4a5b6c7d-8e9f-4a0b-9c1d-2e3f4a5b6c7d");
    const CHEST: Guid = Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d");
    const MOSS: Guid = Guid::parse("2e3d4c5b-6f7a-4b2c-9d3e-4f5a6b7c8d9e");
    const RECIPES: u32 = 1;
    const RECIPE_SIZE: usize = 0xa8;

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
        fn manifest() -> &'static enshwick::abi::Manifest {
            enshwick::manifest!(id: "you.kinds", side: BOTH, join: true)
        }

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

    enshwick::declare_mod!(Kinds);

    /// A registry of one recipe per item in `outputs`, each making one.
    fn registry(outputs: &[Guid]) -> Vec<u8> {
        let mut blob = Blob::new(&[0u8; 16]);
        let first = blob.append(&vec![0; RECIPE_SIZE * outputs.len()], 8);
        for (i, output) in outputs.iter().enumerate() {
            let at = first + i * RECIPE_SIZE;
            let guid = [u8::try_from(i + 1).unwrap(); 16];
            assert!(blob.write(at, &guid));
            let mut stack = vec![0u8; 0x18];
            stack[..16].copy_from_slice(&output.0);
            stack[0x14..0x18].copy_from_slice(&1u32.to_le_bytes());
            let stack_at = blob.append(&stack, 8);
            assert!(blob.set_array(at + 0x30, stack_at, 1));
        }
        assert!(blob.set_array(0, first, u32::try_from(outputs.len()).unwrap()));
        blob.into_bytes()
    }

    /// What each recipe of `bytes` makes and how many.
    fn made(bytes: &[u8]) -> Vec<(Guid, u32)> {
        let blob = Blob::new(bytes);
        let (first, count) = blob.array(0).unwrap();
        (0..count as usize)
            .map(|i| {
                let (stack, _) = blob.array(first + i * RECIPE_SIZE + 0x30).unwrap();
                let mut item = [0u8; 16];
                item.copy_from_slice(&blob.bytes()[stack..stack + 16]);
                (Guid(item), blob.read_u32(stack + 0x14).unwrap())
            })
            .collect()
    }

    fn host() -> FakeHost {
        let mut host = FakeHost::new().type_hash(types::RECIPE_REGISTRY_RESOURCE.id(), RECIPES);
        for (id, size) in [
            ("recipe", 0xa8),
            ("recipe_ingredient", 0x18),
            ("recipe_stack", 0x18),
        ] {
            host = host.type_size(id, size);
        }
        for (id, at) in [
            ("recipe_registry.recipes", 0),
            ("recipe.guid", 0),
            ("recipe.id", 0x10),
            ("recipe.workshop_guid", 0x14),
            ("recipe.workshop", 0x24),
            ("recipe.input", 0x28),
            ("recipe.output", 0x30),
            ("recipe.required_props", 0x38),
            ("recipe.debug_name", 0x40),
            ("recipe.completion", 0x5c),
            ("recipe_stack.item", 0x10),
            ("recipe_stack.count", 0x14),
        ] {
            host = host.constant(id, at);
        }
        host
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
        let key = RECIPE_REGISTRY.key_by_hash(RECIPES);
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
