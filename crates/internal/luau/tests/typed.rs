//! A Luau mod reads and writes a generated component, handles a generated
//! event and serves a generated resource through `enshwick.typed`, as tables
//! by the generated items' Rust names, the same calls a Rust mod makes on
//! `enshwick::typed`.

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use enshwick::abi::{ResourceKey, Status};
    use enshwick::content::Guid;
    use enshwick::resources::{Constant, Field, Kind, Type, fnv1a32};
    use enshwick::test::{FakeHost, Installed};
    use enshwick::{Entity, Load, Mod, Off};
    use enshwick_luau::{Limits, Scripts};

    const FACTION: &str = "keen::ecs::Faction";
    const DIED: &str = "keen::ecs::EntityDiedEvent";
    const MAPPING: &str = "keen::BuildingMaterialMappingResource";
    const COMBAT_FLAGS: &str = "keen::ecs::CombatFlags";
    const TAGS: &str = "keen::ecs::EntityTagContainer";
    const STONE: Guid = Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d");
    const ENTITY: Entity = Entity(4096);

    static CASE: Mutex<Option<Scripts>> = Mutex::new(None);

    struct Luau;

    impl Mod for Luau {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let scripts = CASE.lock().unwrap().take().unwrap();
            let _runtime =
                enshwick_luau::start(load, "you.luau", scripts, None, Limits::default())?;
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Luau,
        r#"
        id = "you.luau"
        side = "both"
        "#
    );

    /// The build's types the generated items read, as server build 479
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
                .with_constants(vec![
                    Constant::new("Vukah", 3),
                    Constant::new("Player", 7),
                    Constant::new("Dragons", 9),
                ]),
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
            Type::new(
                "keen::ObjectReference<keen::ecs::EntityTag>",
                Kind::ObjectReference,
                16,
                4,
            ),
            Type::new("keen::ecs::EntityTagReference", Kind::Typedef, 16, 4)
                .with_inner("keen::ObjectReference<keen::ecs::EntityTag>"),
            Type::new("keen::ecs::EntityTagReferenceEntry", Kind::Struct, 16, 4)
                .with_fields(vec![field("tag", 0, "keen::ecs::EntityTagReference")]),
            Type::new(
                "keen::BlobArray<keen::ecs::EntityTagReferenceEntry>",
                Kind::BlobArray,
                8,
                4,
            )
            .with_inner("keen::ecs::EntityTagReferenceEntry"),
            Type::new(TAGS, Kind::Struct, 8, 4)
                .with_inner("keen::ecs::Component")
                .with_fields(vec![field(
                    "tags",
                    0,
                    "keen::BlobArray<keen::ecs::EntityTagReferenceEntry>",
                )]),
        ]
    }

    fn hash(name: &str) -> u32 {
        fnv1a32(name.as_bytes())
    }

    fn key() -> ResourceKey {
        STONE.key_by_hash(hash(MAPPING))
    }

    /// A tag container's set-up data: an offset of 8 from its one field, a
    /// count of 2, then the two tags.
    fn tags() -> Vec<u8> {
        [
            &8i32.to_le_bytes()[..],
            &2u32.to_le_bytes(),
            &[0x11; 16],
            &[0x22; 16],
        ]
        .concat()
    }

    fn stone() -> Vec<u8> {
        [&[5, 0, 0, 0][..], &[0xab; 16]].concat()
    }

    fn run(main: &str, faction: u8) -> Installed {
        run_on(FakeHost::new().world(), main, faction)
    }

    fn run_on(host: FakeHost, main: &str, faction: u8) -> Installed {
        *CASE.lock().unwrap() = Some(Scripts::new().with("main.luau", main));
        let loader = host
            .types(types())
            .type_hash("ecs::Faction", hash(FACTION))
            .type_size("ecs::Faction", 1)
            .type_hash("ecs::CombatFlags", hash(COMBAT_FLAGS))
            .type_hash("ecs::EntityTagContainer", hash(TAGS))
            .type_size("ecs::EntityTagContainer", 8)
            .component(ENTITY, hash(TAGS), &tags())
            .type_hash("BuildingMaterialMappingResource", hash(MAPPING))
            .game_event_type("ecs::EntityDiedEvent", hash(DIED), 16)
            .component(ENTITY, hash(FACTION), &[faction])
            .resource(key(), stone())
            .install();
        // SAFETY: the three functions are the exports of this file's `declare_mod!(Luau)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        loader
    }

    fn logged(loader: &Installed, text: &str) -> bool {
        loader.logs().iter().any(|(_, line)| line.contains(text))
    }

    #[test]
    fn a_script_reads_and_writes_a_component_by_its_rust_names() {
        let loader = run(
            r#"
            local faction = enshwick.typed.component_type("ecs.Faction")
            local value = faction:read(4096)
            print(`faction {value.faction} base {type(value.base)} none {faction:read(1) == nil}`)
            value.faction = "Vukah"
            faction:write(4096, value)
            print(`now {faction:read(4096).faction}`)
            value.faction = { Other = "Dragons" }
            faction:write(4096, value)
            local ok, why = pcall(faction.write, faction, 4096, { faction = "Vukah", base = {}, rank = 1 })
            print(`stray {ok} {string.find(tostring(why), "rank") ~= nil}`)
        "#,
            7,
        );
        for line in [
            "faction Player base table none true",
            "now Vukah",
            "stray false true",
        ] {
            assert!(logged(&loader, line), "{line}: {:?}", loader.logs());
        }
        assert_eq!(
            loader.component_bytes(ENTITY, hash(FACTION)),
            Some(vec![9]),
            "a constant the enum does not name, written by its name"
        );
    }

    #[test]
    fn a_component_type_opens_on_a_build_without_the_world_reads_and_its_reads_say_why() {
        let loader = run_on(
            FakeHost::new(),
            r#"
            local faction = enshwick.typed.component_type("ecs.Faction")
            local ok, why = pcall(faction.read, faction, 4096)
            print(`read {ok} {why}`)
        "#,
            7,
        );
        let logs = loader.logs();
        let read = logs
            .iter()
            .find_map(|(_, line)| line.split_once("read ").map(|(_, rest)| rest.to_owned()))
            .unwrap_or_else(|| panic!("{logs:?}"));
        assert!(
            read.starts_with("false") && read.contains("typed ecs::Faction"),
            "{read}"
        );
    }

    #[test]
    fn a_constant_the_enum_does_not_name_reads_as_other() {
        let loader = run(
            r#"
            local value = enshwick.typed.component_type("ecs.Faction"):read(4096)
            print(`other {value.faction.Other}`)
        "#,
            9,
        );
        assert!(logged(&loader, "other Dragons"), "{:?}", loader.logs());
    }

    #[test]
    fn a_script_handles_a_generated_event_as_a_table() {
        let loader = run(
            r#"
            enshwick.typed.event_type("ecs.EntityDiedEvent"):on(function(died)
                print(`died {died.dead_entity_id.id} at {died.base.time_stamp.value}`)
            end)
        "#,
            7,
        );
        let mut died = [0u8; 16];
        died[..8].copy_from_slice(&12_i64.to_le_bytes());
        died[8..12].copy_from_slice(&77u32.to_le_bytes());
        assert_eq!(loader.raise_game_event(hash(DIED), &died), 1);
        assert!(logged(&loader, "died 77 at 12"), "{:?}", loader.logs());
    }

    #[test]
    fn a_script_reads_and_serves_a_generated_resource() {
        let loader = run(
            r#"
            local mapping = enshwick.typed.resource_type("BuildingMaterialMappingResource")
            local key = mapping:key("6943053a-32c1-4718-ae22-e61427ddf54d")
            local stone = mapping:game_value(key)
            print(`stone {stone.local_id} material {stone.material_id} type {key.type}`)
            mapping:serve(key, key, function(value)
                value.local_id += 1
                return value
            end)
        "#,
            7,
        );
        let material = Guid([0xab; 16]);
        let line = format!("stone 5 material {material} type {MAPPING}");
        assert!(logged(&loader, &line), "{line}: {:?}", loader.logs());
        let built = loader.build_resource(&key(), &stone()).unwrap();
        assert_eq!(built[0], 6, "local id + 1");
        assert_eq!(built[4..], stone()[4..], "the GUID kept");
    }

    #[test]
    fn a_change_that_returns_nil_serves_the_base_unchanged() {
        let loader = run(
            r#"
            local mapping = enshwick.typed.resource_type("BuildingMaterialMappingResource")
            local key = mapping:key("6943053a-32c1-4718-ae22-e61427ddf54d")
            mapping:serve(key, key, function() return nil end)
        "#,
            7,
        );
        assert_eq!(loader.build_resource(&key(), &stone()).unwrap(), stone());
    }

    #[test]
    fn a_type_whose_shape_changed_or_an_unknown_id_is_refused_by_name() {
        let loader = run(
            r#"
            local ok, why = pcall(enshwick.typed.component_type, "ecs.CombatFlags")
            print(`changed {ok} {why}`)
            ok, why = pcall(enshwick.typed.component_type, "BuildingMaterialMappingResource")
            print(`role {ok} {why}`)
            ok, why = pcall(enshwick.typed.resource_type, "ecs.Nothing")
            print(`unknown {ok} {why}`)
        "#,
            7,
        );
        let logs = loader.logs();
        let line = |prefix: &str| {
            logs.iter()
                .find_map(|(_, line)| line.split_once(prefix).map(|(_, rest)| rest.to_owned()))
                .unwrap_or_else(|| panic!("{prefix}: {logs:?}"))
        };
        let changed = line("changed ");
        assert!(
            changed.starts_with("false")
                && changed.contains("ecs::CombatFlags")
                && changed.contains("immune"),
            "{changed}"
        );
        assert!(line("role ").contains("no generated component"), "{logs:?}");
        assert!(
            line("unknown ").contains("no generated resource"),
            "{logs:?}"
        );
    }

    #[test]
    fn a_script_reads_a_live_component_s_blob_array() {
        let loader = run(
            r#"
            local container = enshwick.typed.component_type("ecs.EntityTagContainer")
            local tags = container:read(4096).tags
            print(`tags {#tags} {tags[1].tag} {tags[2].tag}`)
        "#,
            7,
        );
        let line = format!("tags 2 {} {}", Guid([0x11; 16]), Guid([0x22; 16]));
        assert!(logged(&loader, &line), "{line}: {:?}", loader.logs());
    }
}
