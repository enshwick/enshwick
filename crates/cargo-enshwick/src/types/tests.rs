use super::*;

/// The sample the Rust writer's output is held to, compiled here so its
/// layout checks run; `ENSHWICK_TYPES_SAMPLE=update` rewrites it.
#[expect(
    unreachable_pub,
    dead_code,
    reason = "the sample is a mod's public module, compiled here only to check it"
)]
#[path = "sample.rs"]
mod sample;

fn ty(kind: Kind, size: u32, align: u16) -> Ty {
    Ty {
        kind,
        size,
        align,
        base: None,
        inner: None,
        count: None,
        fields: Vec::new(),
        values: Vec::new(),
    }
}

fn fields(list: &[(&str, u64, Option<&str>)]) -> Vec<Field> {
    list.iter()
        .map(|&(name, offset, ty)| Field {
            name: name.to_owned(),
            offset,
            ty: ty.map(str::to_owned),
        })
        .collect()
}

/// A small build: primitives, a typedef, an enum, an array, a struct that
/// derives from another, a field Rust cannot place, two types of one short
/// name and a designer-data twin.
fn catalogue() -> Catalogue {
    let mut c = Catalogue::new();
    let mut add = |name: &str, ty: Ty| {
        c.insert(name.to_owned(), ty);
    };
    add("keen::bool", ty(Kind::Bool, 1, 1));
    add("keen::uint8", ty(Kind::Uint8, 1, 1));
    add("keen::uint32", ty(Kind::Uint32, 4, 4));
    add("keen::uint64", ty(Kind::Uint64, 8, 8));
    add("keen::float32", ty(Kind::Float32, 4, 4));
    add(
        "keen::HashKey32",
        Ty {
            fields: fields(&[("value", 0, Some("keen::uint32"))]),
            ..ty(Kind::Struct, 4, 4)
        },
    );
    add(
        "keen::ItemId",
        Ty {
            inner: Some("keen::HashKey32".into()),
            ..ty(Kind::Typedef, 4, 4)
        },
    );
    add(
        "keen::EquipmentSlotType",
        Ty {
            values: vec![
                ("Invalid".into(), 0),
                ("Roof".into(), 11),
                ("MagicalFog".into(), 12),
            ],
            ..ty(Kind::Enum, 1, 1)
        },
    );
    add(
        "keen::StaticArray<keen::uint8,4>",
        Ty {
            inner: Some("keen::uint8".into()),
            count: Some(4),
            ..ty(Kind::StaticArray, 4, 1)
        },
    );
    add(
        "keen::ecs::GameEvent",
        Ty {
            fields: fields(&[
                ("time", 0, Some("keen::uint32")),
                ("flags", 4, Some("keen::uint8")),
            ]),
            ..ty(Kind::Struct, 8, 4)
        },
    );
    add(
        "keen::ecs::HitEvent",
        Ty {
            base: Some("keen::ecs::GameEvent".into()),
            fields: fields(&[
                ("damage", 8, Some("keen::float32")),
                ("item", 12, Some("keen::ItemId")),
                ("slots", 16, Some("keen::StaticArray<keen::uint8,4>")),
                ("isCritical", 20, Some("keen::bool")),
                ("type", 21, Some("keen::EquipmentSlotType")),
                ("lost", 22, None),
            ]),
            ..ty(Kind::Struct, 24, 4)
        },
    );
    add(
        "keen::Packed",
        Ty {
            fields: fields(&[("wide", 4, Some("keen::uint64"))]),
            ..ty(Kind::Struct, 12, 4)
        },
    );
    add("keen::ai::State", ty(Kind::Struct, 4, 4));
    add("keen::fish::State", ty(Kind::Struct, 4, 4));
    add(
        "keen::ds::ecs::HitEvent",
        Ty {
            fields: fields(&[("damage", 0, Some("keen::float32"))]),
            ..ty(Kind::Struct, 4, 4)
        },
    );
    c
}

#[test]
fn every_runtime_item_is_chosen_when_none_is_named() {
    let chosen = select(&catalogue(), &[]).unwrap();
    assert!(chosen.contains("keen::ecs::HitEvent"));
    assert!(chosen.contains("keen::ItemId"));
    assert!(!chosen.contains("keen::ds::ecs::HitEvent"));
    assert!(!chosen.contains("keen::uint32"), "a primitive is no item");
}

#[test]
fn a_named_type_brings_every_type_it_needs() {
    let chosen = select(&catalogue(), &["keen::ecs::HitEvent".to_owned()]).unwrap();
    let chosen: Vec<&str> = chosen.iter().map(String::as_str).collect();
    assert_eq!(
        chosen,
        [
            "keen::EquipmentSlotType",
            "keen::HashKey32",
            "keen::ItemId",
            "keen::ecs::GameEvent",
            "keen::ecs::HitEvent",
        ]
    );
}

#[test]
fn a_namespace_names_its_items_and_a_missing_name_is_an_error() {
    let chosen = select(&catalogue(), &["keen::ai::".to_owned()]).unwrap();
    assert_eq!(chosen.into_iter().collect::<Vec<_>>(), ["keen::ai::State"]);
    let error = select(
        &catalogue(),
        &["keen::Nothing".to_owned(), "keen::none::".to_owned()],
    )
    .unwrap_err();
    assert_eq!(
        error.to_string(),
        "no reflected type is named keen::Nothing, keen::none::"
    );
}

#[test]
fn the_rust_module_matches_the_compiled_sample() {
    let catalogue = catalogue();
    let chosen = select(&catalogue, &[]).unwrap();
    let rendered = rust::render(&catalogue, &chosen, "enshrouded_server.exe sample");
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/types/sample.rs");
    if std::env::var("ENSHWICK_TYPES_SAMPLE").as_deref() == Ok("update") {
        std::fs::write(&path, &rendered).unwrap();
    }
    assert_eq!(
        rendered,
        std::fs::read_to_string(&path).unwrap(),
        "run the test with ENSHWICK_TYPES_SAMPLE=update and read the change"
    );
}

#[test]
fn a_struct_from_the_sample_reads_from_game_memory_at_its_offsets() {
    let mut bytes = [0u8; 24];
    bytes[0..4].copy_from_slice(&7u32.to_le_bytes());
    bytes[8..12].copy_from_slice(&2.5f32.to_le_bytes());
    bytes[12..16].copy_from_slice(&0xabcdu32.to_le_bytes());
    bytes[16..20].copy_from_slice(&[1, 2, 3, 4]);
    bytes[20] = 1;
    bytes[21] = 12;
    // SAFETY: `bytes` holds the 24 bytes of a `HitEvent` at offset 0.
    let hit: sample::keen::ecs::HitEvent = unsafe { enshwick::raw::read_at(bytes.as_ptr(), 0) };
    assert_eq!(hit.base.time, 7);
    assert!((hit.damage - 2.5).abs() < f32::EPSILON);
    assert_eq!(hit.item.value, 0xabcd);
    assert_eq!(hit.slots, [1, 2, 3, 4]);
    assert_eq!(hit.is_critical, 1);
    assert_eq!(hit.r#type, sample::keen::EquipmentSlotType::MAGICAL_FOG);
}

#[test]
fn the_luau_definitions_name_types_apart_and_keep_the_game_s_field_names() {
    let catalogue = catalogue();
    let chosen = select(&catalogue, &[]).unwrap();
    insta::assert_snapshot!(luau::render(
        &catalogue,
        &chosen,
        "enshrouded_server.exe sample"
    ));
}

#[test]
fn a_folder_without_the_game_is_named_in_the_error() {
    let dir = std::env::temp_dir().join(format!("enshwick-types-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let error = run(&dir, &[], None, None).unwrap_err().to_string();
    std::fs::remove_dir_all(&dir).unwrap();
    assert!(error.contains("enshrouded_server.exe"), "{error}");
}

#[test]
fn a_game_type_named_as_an_api_type_takes_its_namespace() {
    let mut catalogue = catalogue();
    catalogue.insert(
        "keen::Player".to_owned(),
        Ty {
            fields: fields(&[("item", 0, Some("keen::ItemId"))]),
            ..ty(Kind::Struct, 4, 4)
        },
    );
    let chosen = select(&catalogue, &[]).unwrap();
    let text = luau::render(&catalogue, &chosen, "sample");
    assert!(text.contains("export type Keen_Player = {"), "{text}");
    for name in cargo_enshwick::luau::type_names() {
        assert!(!text.contains(&format!("export type {name} =")), "{name}");
    }
}

#[test]
fn every_game_type_keeps_clear_of_the_api_s_names() {
    let Some(exe) = std::env::var_os("ENSH_SERVER_EXE") else {
        return;
    };
    let text = luau_definitions(Path::new(&exe)).unwrap();
    for name in cargo_enshwick::luau::type_names() {
        assert!(!text.contains(&format!("\nexport type {name} =")), "{name}");
    }
}
