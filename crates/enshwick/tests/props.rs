//! Mods add prop types: an item placing a template copied from a vanilla
//! prop's, and crafting props the loader gives a bit of the shared mask,
//! required by the recipes the mod names.

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use enshwick::abi::{ResourceKey, Status};
    use enshwick::content::items::NewItem;
    use enshwick::content::props::{CraftingPropSetup, NewProp, PropId};
    use enshwick::content::recipes::RECIPE_REGISTRY;
    use enshwick::content::workshops::WORKSHOP_REGISTRY;
    use enshwick::content::{EngineFacts, Guid, Id, types};
    use enshwick::resources::{Registry, Value};
    use enshwick::test::{CRAFTING_PROP_ITEM, FakeHost, PROP_ITEM, VANILLA_CRAFTING_PROPS};
    use enshwick::{ItemId, Load, Mod, Off};

    const CRATE: Id = Id::new("you.props:crate");
    const ANVIL: Id = Id::new("you.props:anvil");
    const KILN: Id = Id::new("you.props:kiln");
    const OVEN: Id = Id::new("you.props:oven");
    const POTION: Guid = Guid::parse("267ab50e-7727-4336-b0e3-bf38e567189f");

    static ADDED: Mutex<Vec<PropId>> = Mutex::new(Vec::new());
    static REFUSED: Mutex<Vec<String>> = Mutex::new(Vec::new());

    fn crafting(item: Id, recipes: Vec<Guid>) -> NewProp {
        NewProp {
            crafting_prop: Some(CraftingPropSetup { recipes }),
            ..NewProp::new(NewItem::new(item, CRAFTING_PROP_ITEM))
        }
    }

    struct Props;

    impl Mod for Props {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let crate_prop = load.add_prop(NewProp::new(NewItem::new(CRATE, PROP_ITEM)))?;
            let anvil = load.add_prop(crafting(ANVIL, vec![POTION]))?;
            let kiln = load.add_prop(crafting(KILN, Vec::new()))?;
            ADDED.lock().unwrap().extend([crate_prop, anvil, kiln]);
            if let Err(off) = load.add_prop(crafting(OVEN, Vec::new())) {
                REFUSED.lock().unwrap().push(off.to_string());
            }
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Props,
        r#"
        id = "you.props"
        side = "both"
        "#
    );

    fn start() -> enshwick::test::Installed {
        let loader = FakeHost::new().props().install();
        // SAFETY: the three functions are the exports of this file's `declare_mod!(Props)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        loader
    }

    /// The resource `key` served, built from `base`, decoded as `type_name`.
    fn built(
        loader: &enshwick::test::Installed,
        key: ResourceKey,
        base: &[u8],
        type_name: &str,
    ) -> Value {
        let bytes = loader.build_resource(&key, base).unwrap();
        Registry::game().unwrap().decode(type_name, &bytes).unwrap()
    }

    fn template(loader: &enshwick::test::Installed, prop: PropId, like: Guid) -> Value {
        let host = enshwick::host().unwrap();
        let hash = host.hash("ecs::TemplateResource").unwrap();
        let base = host.game_resource(like.key_by_hash(hash)).unwrap();
        built(
            loader,
            prop.template.guid().key_by_hash(hash),
            &base,
            "demo::ecs::TemplateResource",
        )
    }

    fn component<'a>(template: &'a Value, name: &str) -> Option<&'a Value> {
        let Some(Value::List(components)) = template.field("components") else {
            return None;
        };
        components.iter().find_map(|held| match held {
            Value::Variant(held) if held.type_name() == name => Some(held.value()),
            _ => None,
        })
    }

    fn id_of(value: Option<&Value>) -> Option<u64> {
        match value?.field("value") {
            Some(Value::Uint(id)) => Some(*id),
            _ => None,
        }
    }

    #[test]
    fn a_prop_item_places_its_own_copy_of_the_vanilla_template() {
        let loader = start();
        let crate_prop = ADDED.lock().unwrap()[0];
        assert_eq!(crate_prop.item, ItemId::from_guid(CRATE.guid()));
        assert_eq!(crate_prop.template.guid(), CRATE.part("template").guid());
        assert_eq!(crate_prop.crafting_prop, None);

        let host = enshwick::host().unwrap();
        let placed_at = host.offset("item_info.placed_entity").unwrap();
        let vanilla = host
            .game_resource(types::ITEM_INFO.key(PROP_ITEM).unwrap())
            .unwrap();
        let item = loader
            .build_resource(&types::ITEM_INFO.key(CRATE.guid()).unwrap(), &vanilla)
            .unwrap();
        assert_eq!(
            item[placed_at..placed_at + 16],
            crate_prop.template.guid().0
        );
        assert_eq!(item[4..20], CRATE.guid().0, "the item is the mod's");

        let copied = template(
            &loader,
            crate_prop,
            Guid::parse("c0ffee00-0000-4000-8000-0000000c4e58"),
        );
        assert_eq!(
            copied.field("name"),
            Some(&Value::Text("Prop_Chest".into()))
        );
    }

    #[test]
    fn crafting_props_take_the_mask_bits_after_the_game_s_and_the_named_recipes_require_them() {
        let loader = start();
        let added = ADDED.lock().unwrap().clone();
        let (anvil, kiln) = (added[1], added[2]);
        let anvil_prop = anvil.crafting_prop.unwrap();
        assert_eq!(anvil_prop, ANVIL.part("crafting-prop").guid());

        let marked = template(
            &loader,
            anvil,
            Guid::parse("c0ffee00-0000-4000-8000-00000000a0a1"),
        );
        let mark = component(&marked, "demo::ecs::RequiredCraftingProp").unwrap();
        assert_eq!(mark.field("type"), Some(&Value::Guid(anvil_prop)));
        assert_eq!(
            id_of(mark.field("craftingPropId")),
            Some(anvil_prop.fnv1a().into())
        );

        let host = enshwick::host().unwrap();
        let key = types::WORKSHOP_REGISTRY_RESOURCE
            .key(WORKSHOP_REGISTRY)
            .unwrap();
        let vanilla = host.game_resource(key).unwrap();
        let registry = built(&loader, key, &vanilla, "demo::WorkshopRegistryResource");
        let Some(Value::List(props)) = registry.field("craftingProps") else {
            panic!("{registry:?}")
        };
        assert_eq!(props.len(), VANILLA_CRAFTING_PROPS + 2);
        for (entry, prop, index) in [
            (
                &props[VANILLA_CRAFTING_PROPS],
                anvil,
                VANILLA_CRAFTING_PROPS,
            ),
            (
                &props[VANILLA_CRAFTING_PROPS + 1],
                kiln,
                VANILLA_CRAFTING_PROPS + 1,
            ),
        ] {
            let guid = prop.crafting_prop.unwrap();
            assert_eq!(entry.field("craftingPropGuid"), Some(&Value::Guid(guid)));
            assert_eq!(
                entry.field("craftingPropIndex"),
                Some(&Value::Uint(index as u64))
            );
            assert_eq!(id_of(entry.field("item")), Some(prop.item.get().into()));
        }

        let key = types::RECIPE_REGISTRY_RESOURCE
            .key(RECIPE_REGISTRY)
            .unwrap();
        let base = FakeHost::recipe_registry(&[(POTION, &[11]), (Guid([3; 16]), &[])]);
        let recipes = built(&loader, key, &base, "demo::RecipeRegistryResource");
        let Some(Value::List(recipes)) = recipes.field("recipes") else {
            panic!("{recipes:?}")
        };
        let Some(Value::List(required)) = recipes[0].field("requiredProps") else {
            panic!("{:?}", recipes[0])
        };
        let required: Vec<_> = required.iter().map(|id| id_of(Some(id))).collect();
        assert_eq!(required, [Some(11), Some(anvil_prop.fnv1a().into())]);
        assert_eq!(
            recipes[0].field("requiredPropsMask"),
            Some(&Value::Uint(1 << VANILLA_CRAFTING_PROPS))
        );
        assert_eq!(
            recipes[1].field("requiredPropsMask"),
            Some(&Value::Uint(0)),
            "not named"
        );

        let refused = REFUSED.lock().unwrap().clone();
        assert_eq!(refused.len(), 1, "{refused:?}");
        assert!(
            refused[0].contains(&OVEN.part("crafting-prop").guid().to_string()),
            "{}",
            refused[0]
        );
    }
}
