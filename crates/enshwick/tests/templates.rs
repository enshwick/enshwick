//! Mods add entity templates as copies of vanilla ones and change the
//! components of a copy or of a vanilla template through the generated
//! types; workshops and item meters are made the same way.

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use enshwick::abi::{Level, ResourceKey, Status};
    use enshwick::content::items::{FuelSetup, METER_TEMPLATE_BASE, NewItem};
    use enshwick::content::templates::{
        ComponentChange, NewTemplate, TEMPLATE_COLLECTION, TemplateId,
    };
    use enshwick::content::workshops::{NewWorkshop, stations};
    use enshwick::content::{Guid, Id};
    use enshwick::resources::{Registry, Value};
    use enshwick::test::{BOSS_CREATURE, CREATURE, FakeHost};
    use enshwick::typed::ecs::{
        Aggro, Boss, ClientCraftingInteraction, Crafting, CraftingInteraction, Faction,
        FuelSetup as Fuel, TemplateResource,
    };
    use enshwick::typed::{Component, FactionType, Reflected};
    use enshwick::{Load, Mod, Off};

    const TAME: Id = Id::new("you.templates:tame-wolf");
    const BOSSY: Id = Id::new("you.templates:boss-wolf");
    const BENCH: Id = Id::new("you.templates:bench");
    const METER: Id = Id::new("you.templates:charges");

    static ADDED: Mutex<Vec<TemplateId>> = Mutex::new(Vec::new());
    static REFUSED: Mutex<Vec<String>> = Mutex::new(Vec::new());

    /// Two content ids of this mod whose game ids are one: the second is a
    /// collision.
    fn clashing() -> (Id, Id) {
        let mut seen = rustc_hash::FxHashMap::default();
        for i in 0u32.. {
            let name: &'static str = Box::leak(format!("you.templates:n{i}").into_boxed_str());
            if let Some(first) = seen.insert(Guid::named(name).fnv1a(), name) {
                return (Id::new(first), Id::new(name));
            }
        }
        unreachable!("a 32-bit hash repeats")
    }

    fn vukah() -> ComponentChange {
        ComponentChange::edit(|faction: &mut Faction| faction.faction = FactionType::Vukah)
    }

    fn refused(result: Result<impl Sized, Off>) {
        if let Err(off) = result {
            REFUSED.lock().unwrap().push(off.to_string());
        }
    }

    struct Templates;

    impl Mod for Templates {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let tame = load.add_template(NewTemplate {
                changes: vec![vukah(), ComponentChange::remove::<Aggro>()],
                ..NewTemplate::new(TAME, CREATURE)
            })?;
            let boss = Boss::from_value(&Value::Struct(Vec::new()))?;
            let bossy = load.add_template(NewTemplate {
                changes: vec![ComponentChange::set(&boss)],
                ..NewTemplate::new(BOSSY, CREATURE)
            })?;
            ADDED.lock().unwrap().extend([tame, bossy]);
            // Two changes of one vanilla template apply in the order served.
            load.change_template(CREATURE, vec![vukah()])?;
            load.change_template(
                CREATURE,
                vec![ComponentChange::edit(|faction: &mut Faction| {
                    assert_eq!(
                        faction.faction,
                        FactionType::Vukah,
                        "after the first change"
                    );
                    faction.faction = FactionType::Player;
                })],
            )?;
            let crafting = Crafting::from_value(&Value::Struct(vec![
                ("workshop".into(), Value::Guid(Guid([1; 16]))),
                (
                    "workshopId".into(),
                    Value::Struct(vec![("value".into(), Value::Uint(1))]),
                ),
            ]))?;
            refused(load.add_template(NewTemplate {
                changes: vec![ComponentChange::set(&crafting)],
                ..NewTemplate::new(Id::new("you.templates:crafting-wolf"), CREATURE)
            }));
            refused(load.change_template(BOSS_CREATURE, vec![ComponentChange::remove::<Aggro>()]));
            refused(load.change_template(Guid([9; 16]), vec![vukah()]));
            let (first, second) = clashing();
            load.add_template(NewTemplate::new(first, CREATURE))?;
            refused(load.add_template(NewTemplate::new(second, CREATURE)));
            load.add_workshop(NewWorkshop::new(BENCH, stations::WORKBENCH))?;
            load.add_item(NewItem {
                meter: Some(FuelSetup {
                    template: METER,
                    max_fuel_charges: 4,
                    start: 1,
                }),
                ..NewItem::new(
                    Id::new("you.templates:charged"),
                    Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d"),
                )
            })?;
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Templates,
        r#"
        id = "you.templates"
        side = "both"
        "#
    );

    /// The fake's template type and its hash.
    fn template_key(guid: Guid) -> ResourceKey {
        guid.key_by_hash(enshwick::resources::fnv1a32(b"demo::ecs::TemplateResource"))
    }

    /// The template `key` served as built from `base`, read as the generated type.
    fn built(loader: &enshwick::test::Installed, key: Guid, base: Guid) -> TemplateResource {
        let host = enshwick::host().unwrap();
        let base = host.game_resource(template_key(base)).unwrap();
        let bytes = loader.build_resource(&template_key(key), &base).unwrap();
        let registry = Registry::game().unwrap();
        TemplateResource::from_value(
            &registry
                .decode("demo::ecs::TemplateResource", &bytes)
                .unwrap(),
        )
        .unwrap()
    }

    /// The template's component of type `T`.
    fn component<T: Component>(template: &TemplateResource) -> Option<T> {
        let name = format!("demo::{}", T::ID);
        template
            .components
            .iter()
            .flatten()
            .find(|held| held.type_name() == name)
            .map(|held| T::from_value(held.value()).unwrap())
    }

    #[test]
    fn templates_are_copied_and_changed_by_component_type() {
        let loader = FakeHost::new().items().workshops().meters().install();
        // SAFETY: the three functions are the exports of this file's `declare_mod!(Templates)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());

        let added = ADDED.lock().unwrap().clone();
        assert_eq!(
            added,
            [TemplateId::new(TAME.guid()), TemplateId::new(BOSSY.guid())]
        );
        let tame = built(&loader, TAME.guid(), CREATURE);
        assert_eq!(
            component::<Faction>(&tame).unwrap().faction,
            FactionType::Vukah
        );
        assert!(component::<Aggro>(&tame).is_none(), "removed");
        assert_eq!(tame.name, "Enemy_Wolf", "the rest of the copy kept");
        let bossy = built(&loader, BOSSY.guid(), CREATURE);
        assert!(
            component::<Boss>(&bossy).is_some(),
            "a component its family has elsewhere"
        );
        let vanilla = built(&loader, CREATURE, CREATURE);
        assert_eq!(
            component::<Faction>(&vanilla).unwrap().faction,
            FactionType::Player,
            "both changes, in order"
        );

        let refused = REFUSED.lock().unwrap().clone();
        assert_eq!(refused.len(), 4, "{refused:?}");
        assert!(
            refused[0].contains("family Npc") && refused[0].contains("ecs::Crafting"),
            "{}",
            refused[0]
        );
        assert!(
            refused[1].contains("no component ecs::Aggro"),
            "{}",
            refused[1]
        );
        assert!(refused[2].contains("no vanilla template"), "{}", refused[2]);
        assert!(
            refused[3].contains("content id"),
            "a collision: {}",
            refused[3]
        );

        let registry = Registry::game().unwrap();
        let empty = registry
            .encode(
                "demo::ecs::TemplateCollectionResource",
                &Value::Struct(vec![("templates".into(), Value::List(Vec::new()))]),
                None,
            )
            .unwrap();
        let collection = TEMPLATE_COLLECTION.key_by_hash(enshwick::resources::fnv1a32(
            b"demo::ecs::TemplateCollectionResource",
        ));
        let listed = registry
            .decode(
                "demo::ecs::TemplateCollectionResource",
                &loader.build_resource(&collection, &empty).unwrap(),
            )
            .unwrap();
        let Some(Value::List(templates)) = listed.field("templates") else {
            panic!("{listed:?}")
        };
        for id in [TAME, BOSSY, BENCH.part("template"), METER] {
            assert!(templates.contains(&Value::Guid(id.guid())), "{id} listed");
        }
    }

    #[test]
    fn naming_a_vanilla_template_first_after_init_warns_once() {
        let loader = FakeHost::new().items().workshops().meters().install();
        // SAFETY: the three functions are the exports of this file's `declare_mod!(Templates)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        let late = |logs: Vec<(Level, String)>| {
            logs.into_iter()
                .filter(|(level, line)| *level == Level::WARN && line.contains("after init"))
                .count()
        };
        assert_eq!(late(loader.logs()), 0, "init named no template");
        let _wolf = enshwick::content::vanilla::template("Enemy_Wolf");
        let _again = enshwick::content::vanilla::template("Enemy_Wolf");
        assert_eq!(late(loader.logs()), 1, "{:?}", loader.logs());
    }

    #[test]
    fn a_workshop_and_a_meter_are_templates_with_their_components_changed() {
        let loader = FakeHost::new().items().workshops().meters().install();
        // SAFETY: the three functions are the exports of this file's `declare_mod!(Templates)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());

        let workshop = BENCH.part("workshop").guid();
        let bench = built(
            &loader,
            BENCH.part("template").guid(),
            stations::WORKBENCH.template,
        );
        let crafting = component::<Crafting>(&bench).unwrap();
        assert_eq!(
            (crafting.workshop, crafting.workshop_id.value),
            (workshop, workshop.fnv1a())
        );
        let menu = component::<CraftingInteraction>(&bench).unwrap();
        assert_eq!(
            (menu.workshop, menu.workshop_id.value),
            (workshop, workshop.fnv1a())
        );
        assert_eq!(
            component::<ClientCraftingInteraction>(&bench)
                .unwrap()
                .workshop_id
                .value,
            workshop.fnv1a()
        );

        let meter = built(&loader, METER.guid(), METER_TEMPLATE_BASE);
        let fuel = component::<Fuel>(&meter).unwrap();
        assert_eq!(fuel.max_fuel_charges, 4);
        assert!((fuel.initial_fuel_percentage - 0.25).abs() < f32::EPSILON);
    }
}
