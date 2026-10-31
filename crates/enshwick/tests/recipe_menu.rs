//! New recipes through the fake loader: the client's crafting menu lists
//! them beside the recipe they copy, and a new workshop gets a menu of its
//! own; the dedicated server, which has no menu, is left alone.

#[cfg(test)]
mod tests {
    use enshwick::abi::{Level, Side, Status};
    use enshwick::content::menu::{MenuLayout, UI_BUNDLE};
    use enshwick::content::recipes::{
        Derived, NewRecipe, RECIPE_REGISTRY, RecipeLayout, derived_guid,
    };
    use enshwick::content::workshops::{NewWorkshop, WorkshopId, stations};
    use enshwick::content::{Blob, EngineFacts, Guid, Id, ResourceType, types};
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
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let bench = load.add_workshop(NewWorkshop::new(
                Id::new("you.menu:bench"),
                stations::WORKBENCH,
            ))?;
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

    enshwick::test::declare_mod!(
        Menu,
        r#"
        id = "you.menu"
        side = "both"
        "#
    );

    /// The fake's reflection for recipes, items, workshops and the crafting
    /// menu.
    fn facts() -> FakeHost {
        FakeHost::new()
            .items()
            .recipes()
            .workshops()
            .crafting_menu()
    }

    /// The hash `facts` gives the resource type `ty`.
    fn hash(ty: ResourceType) -> u32 {
        facts().hash(ty.id()).unwrap()
    }

    /// The vanilla registry: the chest's recipe and a block's, both at the
    /// Carpenter.
    fn registry() -> Vec<u8> {
        let layout = RecipeLayout::resolve(&facts()).unwrap();
        let [_, _, _, debug_name] = layout.relative;
        let mut blob = Blob::new(&[0u8; 16]);
        let first = blob.append(&vec![0; layout.recipe * 2], 8);
        for (i, (guid, output, name)) in [
            (CHEST_RECIPE, CHEST, &b"Prop_Chest"[..]),
            (PLANK_RECIPE, Guid([7; 16]), &b"Block_Plank"[..]),
        ]
        .into_iter()
        .enumerate()
        {
            let at = first + i * layout.recipe;
            assert!(blob.write(at + layout.guid, &guid.0));
            assert!(blob.write(at + layout.id, &guid.fnv1a().to_le_bytes()));
            assert!(blob.write(at + layout.workshop, &CARPENTER.to_le_bytes()));
            let mut stack = vec![0u8; layout.stack_size];
            stack[..16].copy_from_slice(&output.0);
            let stack_at = blob.append(&stack, 8);
            let name_at = blob.append(name, 1);
            assert!(blob.set_array(at + layout.output, stack_at, 1));
            let len = u32::try_from(name.len()).unwrap();
            assert!(blob.set_array(at + debug_name, name_at, len));
        }
        assert!(blob.set_array(layout.recipes, first, 2));
        blob.into_bytes()
    }

    /// The vanilla UI bundle: the Carpenter's menu, one group of one set
    /// with the plank and the chest.
    fn bundle() -> Vec<u8> {
        let layout = MenuLayout::resolve(&facts()).unwrap();
        let menu = layout.collection;
        let mut blob = Blob::new(&vec![0u8; menu + layout.max_sets + 4]);
        let ids: Vec<u8> = [PLANK_RECIPE.fnv1a(), CHEST_RECIPE.fnv1a(), 5]
            .iter()
            .flat_map(|id| id.to_le_bytes())
            .collect();
        let ids = blob.append(&ids, 4);
        let set = blob.append(&vec![0; layout.set], 8);
        assert!(blob.set_array(set + layout.entries, ids, 3));
        let group = blob.append(&vec![1; layout.group], 8);
        assert!(blob.set_array(group + layout.sets, set, 1));
        let tree = blob.append(&vec![0; layout.tree], 8);
        assert!(blob.set_array(tree + layout.groups, group, 1));
        let workshops = blob.append(&CARPENTER.to_le_bytes(), 4);
        assert!(blob.set_array(menu + layout.workshops, workshops, 1));
        assert!(blob.set_array(menu + layout.trees, tree, 1));
        assert!(blob.write(menu + layout.max_groups, &1u32.to_le_bytes()));
        assert!(blob.write(menu + layout.max_sets, &1u32.to_le_bytes()));
        blob.into_bytes()
    }

    /// The ids of each set of each group of each tree, by workshop.
    fn menu(bytes: &[u8]) -> Vec<(u32, Vec<Vec<Vec<u32>>>)> {
        let layout = MenuLayout::resolve(&facts()).unwrap();
        let menu = layout.collection;
        let blob = Blob::new(bytes);
        let (workshops, count) = blob.array(menu + layout.workshops).unwrap();
        let (trees, _) = blob.array(menu + layout.trees).unwrap();
        let array = |field: usize, size: usize| {
            let (start, n) = blob.array(field).unwrap();
            (0..n as usize).map(move |i| start + i * size)
        };
        (0..count as usize)
            .map(|t| {
                let groups = array(trees + t * layout.tree + layout.groups, layout.group)
                    .map(|group| {
                        array(group + layout.sets, layout.set)
                            .map(|set| {
                                array(set + layout.entries, 4)
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

    /// The fake on `side`, without the crafting menu's reflection unless
    /// `menu`.
    fn host(side: Side, menu: bool) -> FakeHost {
        let host = FakeHost::new().items().recipes().workshops();
        let bundle = types::FB_UI_BUNDLE;
        let host = if menu {
            host.crafting_menu()
        } else {
            host.type_hash(bundle.id(), hash(bundle))
        };
        let key = RECIPE_REGISTRY.key_by_hash(hash(types::RECIPE_REGISTRY_RESOURCE));
        host.side(side).resource(key, registry())
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
        let key = UI_BUNDLE.key_by_hash(hash(types::FB_UI_BUNDLE));
        let built = loader.build_resource(&key, &bundle()).unwrap();
        let bench = WorkshopId::new(BENCH.guid()).id();
        let plank = derived_guid(PLANK_RECIPE, fnv1a(b"menu")).fnv1a();
        let chest = derived_guid(CHEST_RECIPE, fnv1a(b"menu")).fnv1a();
        assert_eq!(
            menu(&built),
            [
                (
                    CARPENTER,
                    vec![vec![vec![
                        PLANK_RECIPE.fnv1a(),
                        plank,
                        CHEST_RECIPE.fnv1a(),
                        chest,
                        SAFE_RECIPE.guid().fnv1a(),
                        5
                    ]]]
                ),
                (bench, vec![vec![vec![LOCKER_RECIPE.guid().fnv1a()]]]),
            ]
        );
    }

    #[test]
    fn derived_recipes_copy_only_the_game_s_own_recipes() {
        let loader = start(Side::CLIENT, true);
        let key = RECIPE_REGISTRY.key_by_hash(hash(types::RECIPE_REGISTRY_RESOURCE));
        let built = Blob::new(&loader.build_resource(&key, &registry()).unwrap());
        let (_, count) = built.array(0).unwrap();
        assert_eq!(count, 6, "two vanilla, the Safe, the locker, two copies");
    }

    #[test]
    fn a_client_without_the_menu_s_reflection_still_adds_the_recipes() {
        let loader = start(Side::CLIENT, false);
        let served: Vec<_> = loader.resources().iter().map(|r| r.key).collect();
        assert!(
            served.contains(&RECIPE_REGISTRY.key_by_hash(hash(types::RECIPE_REGISTRY_RESOURCE)))
        );
        assert!(!served.contains(&UI_BUNDLE.key_by_hash(hash(types::FB_UI_BUNDLE))));
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
        let key = UI_BUNDLE.key_by_hash(hash(types::FB_UI_BUNDLE));
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
