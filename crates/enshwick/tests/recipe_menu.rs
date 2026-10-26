//! New recipes through the fake loader: the client's crafting menu lists
//! them beside the recipe they copy, and a new workshop gets a menu of its
//! own; the dedicated server, which has no menu, is left alone.

#[cfg(test)]
mod tests {
    use enshwick::abi::{Level, Side, Status};
    use enshwick::content::menu::UI_BUNDLE;
    use enshwick::content::recipes::{Derived, NewRecipe, RECIPE_REGISTRY, derived_guid};
    use enshwick::content::workshops::{NewWorkshop, WorkshopId, stations};
    use enshwick::content::{Blob, Guid, Id, types};
    use enshwick::test::{FakeHost, Installed};
    use enshwick::{Load, Mod, Off};

    const CHEST: Guid = Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d");
    const CHEST_RECIPE: Guid = Guid::parse("45390726-8a83-4538-91a2-db2a633c8eac");
    const PLANK_RECIPE: Guid = Guid::parse("0b6c5a52-1df4-4a3b-bd0e-6c1f7e2a9c44");
    const CARPENTER: u32 = 3_185_407_206;
    const SAFE_RECIPE: Id = Id::new("you.menu:safe.recipe");
    const LOCKER_RECIPE: Id = Id::new("you.menu:locker.recipe");
    const BENCH: Id = Id::new("you.menu:bench.workshop");

    struct Menu;

    impl Mod for Menu {
        fn manifest() -> &'static enshwick::abi::Manifest {
            enshwick::manifest!(id: "you.menu", side: BOTH)
        }

        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let bench = load.add_workshop(NewWorkshop {
                like: stations::WORKBENCH,
                item: Id::new("you.menu:bench"),
                template: Id::new("you.menu:bench.template"),
                workshop: BENCH,
                name: None,
                description: None,
                station_recipe: None,
                debug_name: "Bench".into(),
            })?;
            load.add_recipe(NewRecipe {
                id: SAFE_RECIPE,
                like: CHEST,
                output: Guid::named("you.menu:safe"),
                output_count: 1,
                cost: 2,
                workshop: None,
            })?;
            load.add_recipe_with(move || NewRecipe {
                id: LOCKER_RECIPE,
                like: CHEST,
                output: Guid::named("you.menu:locker"),
                output_count: 1,
                cost: 3,
                workshop: Some(bench),
            })?;
            load.derive_recipes("menu", |recipe| {
                let name = recipe.debug_name();
                (name.starts_with("Block_") || name == "Prop_Chest").then_some(Derived {
                    output: 2,
                    cost: 1,
                    workshop: None,
                })
            })?;
            Ok(Self)
        }
    }

    enshwick::declare_mod!(Menu);

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

    const RECIPES: u32 = 1;
    const BUNDLE: u32 = 2;

    fn host(side: Side, menu: bool) -> FakeHost {
        let mut host = FakeHost::new()
            .side(side)
            .resource(RECIPE_REGISTRY.key_by_hash(RECIPES), registry());
        for (ty, hash) in [
            (types::RECIPE_REGISTRY, RECIPES),
            (types::UI_BUNDLE, BUNDLE),
            (types::ITEM_INFO, 3),
            (types::ITEM_REGISTRY, 4),
            (types::TEMPLATE, 5),
            (types::TEMPLATE_COLLECTION, 6),
            (types::WORKSHOP_REGISTRY, 7),
        ] {
            host = host.type_hash(ty.id(), hash);
        }
        for (id, hash) in [
            ("crafting", 8),
            ("crafting_interaction", 9),
            ("client_crafting_interaction", 10),
        ] {
            host = host.type_hash(id, hash);
        }
        for (id, size) in [
            ("recipe", 0xa8),
            ("recipe_ingredient", 0x18),
            ("recipe_stack", 0x18),
            ("item_stat", 0x28),
            ("crafting", 0x18),
            ("crafting_interaction", 0x18),
            ("client_crafting_interaction", 0x8),
            ("workshop", 0x40),
            ("crafting_menu_tree", 8),
            ("crafting_menu_group", 0x38),
            ("crafting_menu_set", 0x28),
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
            ("item_info.id", 0),
            ("item_info.object", 4),
            ("item_info.name", 0x48),
            ("item_info.icon_image", 0x58),
            ("item_info.description", 0x68),
            ("item_info.flags", 0x78),
            ("item_info.pid_entity", 0x80),
            ("item_info.ui_values", 0x90),
            ("item_info.placed_entity", 0xb0),
            ("item_stat.label", 0),
            ("item_stat.format", 4),
            ("item_stat.value", 8),
            ("item_stat.signed", 0x24),
            ("item_registry.refs", 0),
            ("item_registry.names", 0x38),
            ("template.components", 0x10),
            ("crafting.workshop", 0),
            ("crafting.workshop_id", 0x10),
            ("crafting_interaction.workshop", 0),
            ("crafting_interaction.workshop_id", 0x10),
            ("client_crafting_interaction.workshop_id", 0),
            ("template_collection.templates", 0),
            ("workshop_registry.workshops", 0),
            ("workshop.guid", 0),
            ("workshop.id", 0x10),
            ("workshop.name", 0x14),
            ("workshop.item", 0x24),
            ("workshop.item_ref", 0x34),
            ("workshop.npc", 0x3c),
        ] {
            host = host.constant(id, at);
        }
        if !menu {
            return host;
        }
        for (id, at) in [
            ("ui_bundle.crafting_menu", 0x40),
            ("crafting_menu.workshops", 0),
            ("crafting_menu.trees", 8),
            ("crafting_menu.max_groups", 0x10),
            ("crafting_menu.max_sets", 0x14),
            ("crafting_menu_tree.groups", 0),
            ("crafting_menu_group.id", 0),
            ("crafting_menu_group.sets", 0x30),
            ("crafting_menu_set.entries", 0x20),
        ] {
            host = host.constant(id, at);
        }
        host
    }

    fn start(side: Side, menu: bool) -> Installed {
        let loader = host(side, menu).install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Menu)`.
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
        let loader = start(Side::CLIENT, true);
        let key = UI_BUNDLE.key_by_hash(BUNDLE);
        let built = loader.build_resource(&key, &bundle()).unwrap();
        let bench = WorkshopId::new(BENCH.guid()).id();
        let plank = derived_guid(PLANK_RECIPE, fnv1a(b"menu")).id();
        let chest = derived_guid(CHEST_RECIPE, fnv1a(b"menu")).id();
        assert_eq!(
            menu(&built),
            [
                (
                    CARPENTER,
                    vec![vec![vec![
                        PLANK_RECIPE.id(),
                        plank,
                        CHEST_RECIPE.id(),
                        chest,
                        SAFE_RECIPE.guid().id(),
                        5
                    ]]]
                ),
                (bench, vec![vec![vec![LOCKER_RECIPE.guid().id()]]]),
            ]
        );
    }

    #[test]
    fn derived_recipes_copy_only_the_game_s_own_recipes() {
        let loader = start(Side::CLIENT, true);
        let key = RECIPE_REGISTRY.key_by_hash(RECIPES);
        let built = Blob::new(&loader.build_resource(&key, &registry()).unwrap());
        let (_, count) = built.array(0).unwrap();
        assert_eq!(count, 6, "two vanilla, the Safe, the locker, two copies");
    }

    #[test]
    fn a_client_without_the_menu_s_reflection_still_adds_the_recipes() {
        let loader = start(Side::CLIENT, false);
        let served: Vec<_> = loader.resources().iter().map(|r| r.key).collect();
        assert!(served.contains(&RECIPE_REGISTRY.key_by_hash(RECIPES)));
        assert!(!served.contains(&UI_BUNDLE.key_by_hash(BUNDLE)));
        assert!(
            loader
                .logs()
                .iter()
                .any(|(level, line)| *level == Level::WARN
                    && line.contains("stay out of the crafting menu")),
            "{:?}",
            loader.logs()
        );
    }

    #[test]
    fn the_dedicated_server_serves_no_menu() {
        let loader = start(Side::SERVER, true);
        let key = UI_BUNDLE.key_by_hash(BUNDLE);
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
