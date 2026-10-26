//! A mod changes recipes by the kind of item they make, as the loader
//! answers it; a recipe whose item the game has not loaded has no kind yet.

#[cfg(test)]
mod tests {
    use enshwick::abi::Status;
    use enshwick::content::recipes::{RECIPE_REGISTRY, RecipeKind};
    use enshwick::content::{Blob, Guid, types};
    use enshwick::test::FakeHost;
    use enshwick::{Load, Mod, Off};

    const PLANK: Guid = Guid::parse("0b6c5a52-1df4-4a3b-bd0e-6c1f7e2a9c44");
    const ROOF: Guid = Guid::parse("9340b525-b047-4d87-a228-52a56b3b9401");
    const CHEST: Guid = Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d");
    const MOSS: Guid = Guid::parse("953f5b06-d529-4610-b37c-c0163b9fd6b0");
    const RECIPES: u32 = 1;
    const RECIPE_SIZE: usize = 0xa8;

    struct Kinds;

    impl Mod for Kinds {
        fn manifest() -> &'static enshwick::abi::Manifest {
            enshwick::manifest!(id: "you.kinds", side: BOTH)
        }

        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            load.modify_recipes(|recipe| match recipe.view().kind() {
                Some(RecipeKind::BuildingBlock) => recipe.scale_output(2),
                Some(RecipeKind::Roof) => recipe.scale_output(3),
                Some(RecipeKind::Other) => recipe.scale_output(5),
                _ => {}
            })?;
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
            let mut stack = vec![0u8; 0x18];
            stack[..16].copy_from_slice(&output.0);
            stack[0x14..0x18].copy_from_slice(&1u32.to_le_bytes());
            let stack_at = blob.append(&stack, 8);
            assert!(blob.set_array(at + 0x30, stack_at, 1));
        }
        assert!(blob.set_array(0, first, u32::try_from(outputs.len()).unwrap()));
        blob.into_bytes()
    }

    /// How many each recipe of `bytes` makes.
    fn made(bytes: &[u8]) -> Vec<u32> {
        let blob = Blob::new(bytes);
        let (first, count) = blob.array(0).unwrap();
        (0..count as usize)
            .map(|i| {
                let (stack, _) = blob.array(first + i * RECIPE_SIZE + 0x30).unwrap();
                blob.read_u32(stack + 0x14).unwrap()
            })
            .collect()
    }

    fn host() -> FakeHost {
        let mut host = FakeHost::new().type_hash(types::RECIPE_REGISTRY.id(), RECIPES);
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
    fn recipes_change_by_the_kind_of_item_they_make_once_it_is_known() {
        let loader = host()
            .recipe_kind(PLANK, RecipeKind::BuildingBlock)
            .recipe_kind(ROOF, RecipeKind::Roof)
            .recipe_kind(MOSS, RecipeKind::Other)
            .install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Kinds)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let key = RECIPE_REGISTRY.key_by_hash(RECIPES);
        let built = loader
            .build_resource(&key, &registry(&[PLANK, ROOF, MOSS, CHEST]))
            .unwrap();
        assert_eq!(made(&built), [2, 3, 5, 1]);
    }
}
