//! The generated typed API reads and writes a component, handles an event
//! and serves a resource by the game's field names, and turns off only a
//! type whose shape the build changed.

#[cfg(test)]
mod tests {
    use std::fmt::Write as _;
    use std::sync::{Mutex, OnceLock};

    use enshwick::abi::{ResourceKey, Status};
    use enshwick::content::Guid;
    use enshwick::resources::{Constant, Field, Kind, Type, fnv1a32};
    use enshwick::typed::ecs::{CombatFlags, EntityDiedEvent, Faction};
    use enshwick::typed::{
        BuildingMaterialMappingResource, ComponentType, EventType, FactionType, ResourceType,
    };
    use enshwick::world::World;
    use enshwick::{Entity, Load, Mod, Off};

    const FACTION: &str = "keen::ecs::Faction";
    const DIED: &str = "keen::ecs::EntityDiedEvent";
    const MAPPING: &str = "keen::BuildingMaterialMappingResource";
    const COMBAT_FLAGS: &str = "keen::ecs::CombatFlags";
    const STONE: Guid = Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d");

    type Resolved = (
        World,
        ComponentType<Faction>,
        ResourceType<BuildingMaterialMappingResource>,
    );

    static RESOLVED: OnceLock<Resolved> = OnceLock::new();
    static DEAD: Mutex<Vec<u32>> = Mutex::new(Vec::new());
    static REFUSED: Mutex<String> = Mutex::new(String::new());

    /// The build's types the four generated items read, as server build 479
    /// reflects them, but for `CombatFlags`, whose first field this build
    /// renamed.
    fn types() -> Vec<Type<'static>> {
        let field = |name, offset, ty| Field::new(name, offset, ty);
        vec![
            Type::new("keen::uint8", Kind::Uint8, 1, 1),
            Type::new("keen::uint32", Kind::Uint32, 4, 4),
            Type::new("keen::sint64", Kind::Sint64, 8, 8),
            Type::new("keen::bool", Kind::Bool, 1, 1),
            Type::new("keen::Guid", Kind::Guid, 16, 4),
            Type::new("keen::ecs::Component", Kind::Struct, 1, 1),
            Type::new("keen::ecs::Event", Kind::Struct, 1, 1),
            Type::new("keen::FactionType", Kind::Enum, 1, 1)
                .with_inner("keen::uint8")
                .with_constants(vec![Constant::new("Vukah", 3), Constant::new("Player", 7)]),
            Type::new(FACTION, Kind::Struct, 1, 1)
                .with_inner("keen::ecs::Component")
                .with_fields(vec![field("faction", 0, "keen::FactionType")]),
            Type::new("keen::Time", Kind::Struct, 8, 8).with_fields(vec![field(
                "value",
                0,
                "keen::sint64",
            )]),
            Type::new("keen::ecs::GameEvent", Kind::Struct, 8, 8)
                .with_inner("keen::ecs::Event")
                .with_fields(vec![field("timeStamp", 0, "keen::Time")]),
            Type::new("keen::ecs::EntityId", Kind::Struct, 4, 4).with_fields(vec![field(
                "id",
                0,
                "keen::uint32",
            )]),
            Type::new(DIED, Kind::Struct, 16, 8)
                .with_inner("keen::ecs::GameEvent")
                .with_fields(vec![field("deadEntityId", 8, "keen::ecs::EntityId")]),
            Type::new(MAPPING, Kind::Struct, 20, 4).with_fields(vec![
                field("localId", 0, "keen::uint8"),
                field("materialId", 4, "keen::Guid"),
            ]),
            Type::new(COMBAT_FLAGS, Kind::Struct, 1, 1)
                .with_inner("keen::ecs::Component")
                .with_fields(vec![field("isImmune", 0, "keen::bool")]),
        ]
    }

    struct Typed;

    impl Mod for Typed {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let refused = ComponentType::<CombatFlags>::resolve(load).unwrap_err();
            *REFUSED.lock().unwrap() = refused.to_string();
            let faction = ComponentType::<Faction>::resolve(load)?;
            EventType::<EntityDiedEvent>::resolve(load)?.on(load, |died| {
                DEAD.lock().unwrap().push(died.dead_entity_id.id);
            })?;
            let mapping = ResourceType::<BuildingMaterialMappingResource>::resolve(load)?;
            let key = mapping.key(STONE);
            let foreign = ResourceKey {
                type_hash: !key.type_hash,
                ..key
            };
            let refused = mapping.serve(load, foreign, key, |_| {}).unwrap_err();
            write!(REFUSED.lock().unwrap(), "\n{refused}").unwrap();
            mapping.serve(load, key, key, |stone| stone.local_id += 1)?;
            let _ = RESOLVED.set((load.world()?, faction, mapping));
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Typed,
        r#"
        id = "you.typed"
        side = "server"
        "#
    );

    #[test]
    fn a_component_an_event_and_a_resource_read_and_write_by_field_name() {
        let entity = Entity(4096);
        let hash = |name: &str| fnv1a32(name.as_bytes());
        let key = ResourceKey {
            guid: STONE.0,
            type_hash: hash(MAPPING),
            part: 0,
        };
        let stone = [&[5, 0, 0, 0][..], &[0xab; 16]].concat();
        let loader = enshwick::test::FakeHost::new()
            .world()
            .types(types())
            .type_hash("ecs::Faction", hash(FACTION))
            .type_size("ecs::Faction", 1)
            .type_hash("ecs::CombatFlags", hash(COMBAT_FLAGS))
            .type_hash("BuildingMaterialMappingResource", hash(MAPPING))
            .game_event_type("ecs::EntityDiedEvent", hash(DIED), 16)
            .component(entity, hash(FACTION), &[7])
            .resource(key, stone.clone())
            .install();
        // SAFETY: the three functions are the exports of this file's `declare_mod!(Typed)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let refused = REFUSED.lock().unwrap().clone();
        assert!(
            refused.contains("ecs::CombatFlags") && refused.contains("immune"),
            "{refused}"
        );

        let (world, faction, mapping) = RESOLVED.get().unwrap();
        let mut value = faction.read(*world, entity).unwrap().unwrap();
        assert_eq!(value.faction, FactionType::Player);
        value.faction = FactionType::Vukah;
        faction.write(*world, entity, &value).unwrap();
        assert_eq!(loader.component_bytes(entity, hash(FACTION)), Some(vec![3]));
        assert_eq!(faction.read(*world, Entity(1)).unwrap(), None);

        let mut died = [0u8; 16];
        died[8..12].copy_from_slice(&77u32.to_le_bytes());
        assert_eq!(loader.raise_game_event(hash(DIED), &died), 1);
        assert_eq!(*DEAD.lock().unwrap(), [77]);

        assert_eq!(mapping.game_value(key).unwrap().local_id, 5);
        let built = loader.build_resource(&key, &stone).unwrap();
        assert_eq!(built[0], 6, "local id + 1");
        assert_eq!(built[4..], stone[4..], "the GUID kept");
        assert_eq!(
            loader.build_resource(&key, &[5]),
            Ok(vec![5]),
            "a base that does not read is served unchanged"
        );
        assert!(refused.contains("a key of type"), "{refused}");
    }
}
