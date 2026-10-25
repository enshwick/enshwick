//! New recipes through the fake loader: the client's crafting menu lists
//! them beside the recipe they copy, and a new workshop gets a menu of its
//! own; the dedicated server, which has no menu, is left alone.

#[cfg(test)]
mod tests {
    use enshwick_loader_api::abi::{Side, Status};
    use enshwick_loader_api::content::menu::UI_BUNDLE;
    use enshwick_loader_api::content::recipes::{
        Derived, NewRecipe, RECIPE_REGISTRY, derived_guid,
    };
    use enshwick_loader_api::content::workshops::{NewWorkshop, WorkshopId, stations};
    use enshwick_loader_api::content::{Blob, Guid, Id, types};
    use enshwick_loader_api::test::{FakeHost, Installed};
    use enshwick_loader_api::{Load, Mod, Off};

    const CHEST: Guid = Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d");
    const CHEST_RECIPE: Guid = Guid::parse("45390726-8a83-4538-91a2-db2a633c8eac");
    const PLANK_RECIPE: Guid = Guid::parse("0b6c5a52-1df4-4a3b-bd0e-6c1f7e2a9c44");
    const CARPENTER: u32 = 3_185_407_206;
    const SAFE_RECIPE: Id = Id::new("menu:safe.recipe");
    const LOCKER_RECIPE: Id = Id::new("menu:locker.recipe");
    const BENCH: Id = Id::new("menu:bench.workshop");

    struct Menu;

    impl Mod for Menu {
        fn manifest() -> &'static enshwick_loader_api::abi::Manifest {
            enshwick_loader_api::manifest!(id: "menu", side: BOTH)
        }

        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let bench = load.add_workshop(NewWorkshop {
                like: stations::WORKBENCH,
                item: Id::new("menu:bench"),
                template: Id::new("menu:bench.template"),
                workshop: BENCH,
                name: None,
                description: None,
                station_recipe: None,
                debug_name: "Bench".into(),
            })?;
            load.add_recipe(NewRecipe {
                id: SAFE_RECIPE,
                like: CHEST,
                output: Guid::named("menu:safe"),
                output_count: 1,
                cost: 2,
                workshop: None,
            })?;
            load.add_recipe_with(move || NewRecipe {
                id: LOCKER_RECIPE,
                like: CHEST,
                output: Guid::named("menu:locker"),
                output_count: 1,
                cost: 3,
                workshop: Some(bench),
            })?;
            load.derive_recipes("menu", |recipe| {
                recipe
                    .debug_name()
                    .starts_with("Block_")
                    .then_some(Derived {
                        output: 2,
                        cost: 1,
                        workshop: None,
                    })
            })?;
            Ok(Self)
        }
    }

    enshwick_loader_api::declare_mod!(Menu);

    /// The vanilla registry: the chest's recipe and a block's, both at the
    /// Carpenter.
    fn registry() -> Vec<u8> {
        let mut blob = Blob::new(&[0u8; 16]);
        let first = blob.append(&[0; 0xa8 * 2], 8);
        for (i, (guid, output, name)) in [
            (CHEST_RECIPE, CHEST, &b"Prop_Chest"[..]),
            (PLANK_RECIPE, Guid([7; 16]), &b"Block_Plank"[..]),
        ]
        .into_iter()
        .enumerate()
        {
            let at = first + i * 0xa8;
            assert!(blob.write(at, &guid.0));
            assert!(blob.write(at + 0x10, &guid.id().to_le_bytes()));
            assert!(blob.write(at + 0x24, &CARPENTER.to_le_bytes()));
            let mut stack = vec![0u8; 0x18];
            stack[..16].copy_from_slice(&output.0);
            let stack_at = blob.append(&stack, 8);
            let name_at = blob.append(name, 1);
            assert!(blob.set_array(at + 0x30, stack_at, 1));
            assert!(blob.set_array(at + 0x40, name_at, u32::try_from(name.len()).unwrap()));
        }
        assert!(blob.set_array(0, first, 2));
        blob.into_bytes()
    }

    /// The vanilla UI bundle: the Carpenter's menu, one group of one set
    /// with the plank and the chest.
    fn bundle() -> Vec<u8> {
        let mut blob = Blob::new(&[0u8; 0x60]);
        let ids: Vec<u8> = [PLANK_RECIPE.id(), CHEST_RECIPE.id(), 5]
            .iter()
            .flat_map(|id| id.to_le_bytes())
            .collect();
        let ids = blob.append(&ids, 4);
        let set = blob.append(&[0; 0x28], 8);
        assert!(blob.set_array(set + 0x20, ids, 3));
        let group = blob.append(&[1; 0x38], 8);
        assert!(blob.set_array(group + 0x30, set, 1));
        let tree = blob.append(&[0; 8], 8);
        assert!(blob.set_array(tree, group, 1));
        let workshops = blob.append(&CARPENTER.to_le_bytes(), 4);
        assert!(blob.set_array(0x40, workshops, 1));
        assert!(blob.set_array(0x48, tree, 1));
        assert!(blob.write(0x50, &1u32.to_le_bytes()));
        assert!(blob.write(0x54, &1u32.to_le_bytes()));
        blob.into_bytes()
    }

    /// The ids of each set of each group of each tree, by workshop.
    fn menu(bytes: &[u8]) -> Vec<(u32, Vec<Vec<Vec<u32>>>)> {
        let blob = Blob::new(bytes);
        let (workshops, count) = blob.array(0x40).unwrap();
        let (trees, _) = blob.array(0x48).unwrap();
        let array = |field: usize, size: usize| {
            let (start, n) = blob.array(field).unwrap();
            (0..n as usize).map(move |i| start + i * size)
        };
        (0..count as usize)
            .map(|t| {
                let groups = array(trees + t * 8, 0x38)
                    .map(|group| {
                        array(group + 0x30, 0x28)
                            .map(|set| {
                                array(set + 0x20, 4)
                                    .map(|at| blob.read_u32(at).unwrap())
                                    .collect()
                            })
                            .collect()
                    })
                    .collect();
                (blob.read_u32(workshops + t * 4).unwrap(), groups)
            })
            .collect()
    }

    fn host(side: Side) -> FakeHost {
        let mut host = FakeHost::new()
            .side(side)
            .resource(RECIPE_REGISTRY.key(types::RECIPE_REGISTRY), registry());
        for (ty, size) in [
            ("keen::RecipeInfo", 0xa8),
            ("keen::RecipeInputResource", 0x18),
            ("keen::RecipeItemStackResource", 0x18),
            ("keen::ImpactUiValue", 0x28),
            ("keen::ecs::Crafting", 0x18),
            ("keen::ecs::CraftingInteraction", 0x18),
            ("keen::ecs::ClientCraftingInteraction", 0x8),
            ("keen::WorkshopResource", 0x40),
            ("keen::FbUiCraftingRecipeTree", 8),
            ("keen::FbUiCraftingRecipeGroup", 0x38),
            ("keen::FbUiCraftingRecipeSet", 0x28),
        ] {
            host = host.type_size(ty, size);
        }
        for (ty, field, at) in [
            ("keen::RecipeRegistryResource", "recipes", 0),
            ("keen::RecipeInfo", "recipeGuid", 0),
            ("keen::RecipeInfo", "recipeId", 0x10),
            ("keen::RecipeInfo", "workshopGuid", 0x14),
            ("keen::RecipeInfo", "workshopId", 0x24),
            ("keen::RecipeInfo", "input", 0x28),
            ("keen::RecipeInfo", "output", 0x30),
            ("keen::RecipeInfo", "requiredProps", 0x38),
            ("keen::RecipeInfo", "debugName", 0x40),
            ("keen::RecipeInfo", "completionRequirementQuery", 0x5c),
            ("keen::RecipeItemStackResource", "item", 0x10),
            ("keen::RecipeItemStackResource", "count", 0x14),
            ("keen::ItemInfo", "itemId", 0),
            ("keen::ItemInfo", "objectId", 4),
            ("keen::ItemInfo", "name", 0x48),
            ("keen::ItemInfo", "iconImage", 0x58),
            ("keen::ItemInfo", "description", 0x68),
            ("keen::ItemInfo", "flags", 0x78),
            ("keen::ItemInfo", "pidEntity", 0x80),
            ("keen::ItemInfo", "uiValues", 0x90),
            ("keen::ItemInfo", "equipment", 0xa0),
            ("keen::ImpactUiValue", "locaId", 0),
            ("keen::ImpactUiValue", "valueFormat", 4),
            ("keen::ImpactUiValue", "value", 8),
            ("keen::ImpactUiValue", "isSigned", 0x24),
            ("keen::ItemRegistryResource", "itemRefs", 0),
            ("keen::ItemRegistryResource", "dbgNames", 0x38),
            ("keen::EquipmentSetup", "placedEntity", 0x10),
            ("keen::ecs::TemplateResource", "components", 0x10),
            ("keen::ecs::Crafting", "workshop", 0),
            ("keen::ecs::Crafting", "workshopId", 0x10),
            ("keen::ecs::CraftingInteraction", "workshop", 0),
            ("keen::ecs::CraftingInteraction", "workshopId", 0x10),
            ("keen::ecs::ClientCraftingInteraction", "workshopId", 0),
            ("keen::ecs::TemplateCollectionResource", "templates", 0),
            ("keen::WorkshopRegistryResource", "workshops", 0),
            ("keen::WorkshopResource", "workshopGuid", 0),
            ("keen::WorkshopResource", "workshopId", 0x10),
            ("keen::WorkshopResource", "name", 0x14),
            ("keen::WorkshopResource", "item", 0x24),
            ("keen::WorkshopResource", "itemRef", 0x34),
            ("keen::WorkshopResource", "isNpc", 0x3c),
            ("keen::FbUiBundle", "menu", 0x30),
            ("keen::FbUiMenu", "crafting", 0x8),
            ("keen::FbUiCrafting", "recipes", 0x8),
            ("keen::FbUiCraftingRecipeCollection", "workshops", 0),
            ("keen::FbUiCraftingRecipeCollection", "trees", 8),
            (
                "keen::FbUiCraftingRecipeCollection",
                "maxRecipeGroupCount",
                0x10,
            ),
            (
                "keen::FbUiCraftingRecipeCollection",
                "maxRecipeSetCount",
                0x14,
            ),
            ("keen::FbUiCraftingRecipeTree", "groups", 0),
            ("keen::FbUiCraftingRecipeGroup", "groupId", 0),
            ("keen::FbUiCraftingRecipeGroup", "sets", 0x30),
            ("keen::FbUiCraftingRecipeSet", "entries", 0x20),
        ] {
            host = host.field_offset(ty, field, at);
        }
        host
    }

    fn start(side: Side) -> Installed {
        let loader = host(side).install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(
            status,
            Status::OK,
            "{:?} {:?}",
            loader.logs(),
            loader.status()
        );
        loader
    }

    #[test]
    fn the_client_menu_lists_new_recipes_beside_the_ones_they_copy() {
        let loader = start(Side::CLIENT);
        let key = UI_BUNDLE.key(types::UI_BUNDLE);
        let built = loader.build_resource(&key, &bundle()).unwrap();
        let bench = WorkshopId::new(BENCH.guid()).id();
        let derived = derived_guid(PLANK_RECIPE, fnv1a(b"menu")).id();
        assert_eq!(
            menu(&built),
            [
                (
                    CARPENTER,
                    vec![vec![vec![
                        PLANK_RECIPE.id(),
                        derived,
                        CHEST_RECIPE.id(),
                        SAFE_RECIPE.guid().id(),
                        5
                    ]]]
                ),
                (bench, vec![vec![vec![LOCKER_RECIPE.guid().id()]]]),
            ]
        );
    }

    #[test]
    fn the_dedicated_server_serves_no_menu() {
        let loader = start(Side::SERVER);
        let key = UI_BUNDLE.key(types::UI_BUNDLE);
        assert!(loader.resources().iter().all(|r| r.key != key));
        assert_eq!(loader.build_resource(&key, &bundle()), Err(Status::MISSING));
    }

    /// FNV-1a, the salt `derive_recipes` makes of the mod id.
    fn fnv1a(bytes: &[u8]) -> u32 {
        bytes.iter().fold(0x811c_9dc5_u32, |hash, &b| {
            (hash ^ u32::from(b)).wrapping_mul(0x0100_0193)
        })
    }
}
