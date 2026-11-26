use enshwick_core::abi::{Header, TerrainMaterialInfo};

use super::*;

pub(super) const SAND: u8 = 9;
pub(super) const ROCK: u8 = 24;
/// The mod the tests add materials as.
pub(super) const MOD: ModIndex = ModIndex::new(1);

pub(super) fn guid(n: u8) -> Guid {
    Guid([n; 16])
}

fn id(n: u64) -> Value {
    Value::Struct(vec![(MATERIAL_ID_VALUE.into(), Value::Uint(n))])
}

/// A palette world naming terrain ids 1 to 116 by GUIDs 1 to 116, as the
/// game's (data 1287369), and the building ids after.
fn world() -> Value {
    let guids = (0..=u8::MAX)
        .map(|n| {
            let named = (1..=116).contains(&n) || n >= 128;
            Value::Guid(if named { guid(n) } else { Guid::NIL })
        })
        .collect();
    Value::Struct(vec![
        ("type".into(), Value::Name("Solid".into())),
        (MATERIAL_GUIDS.into(), Value::List(guids)),
    ])
}

fn config(n: u64) -> Value {
    Value::Struct(vec![
        (TERRAIN_MATERIAL.into(), id(n)),
        (TERRAIN_ITEM.into(), Value::Guid(guid(200))),
        (HARDNESS.into(), Value::Name("Soft".into())),
        (HEALTH_POINTS.into(), Value::Int(3)),
        (CAN_BE_FLATTENED.into(), Value::Bool(true)),
        ("frictionFactor".into(), Value::Float(0.5)),
    ])
}

fn rules() -> Value {
    Value::Struct(vec![(
        TERRAIN_CONFIGS.into(),
        Value::List((1..=116).map(config).collect()),
    )])
}

pub(super) fn palette() -> Palette {
    Palette::read([&world()], &rules()).unwrap()
}

pub(super) fn request(n: u8) -> TerrainMaterialAdd {
    TerrainMaterialAdd {
        header: Header::of::<TerrainMaterialAdd>(),
        guid: guid(n),
        item: Guid::NIL,
        health: 0,
        like: SAND,
        fallback: SAND,
        changes: 0,
        hardness: 0,
        flags: 0,
        reserved: [0; 3],
    }
}

/// Adds the requests in turn, as mods in load order.
fn add_all(requests: &[TerrainMaterialAdd]) -> (Vec<Material>, Vec<TerrainMaterialError>) {
    let palette = palette();
    let (mut added, mut refused) = (Vec::new(), Vec::new());
    for request in requests {
        match place(&palette, &added, MOD, request) {
            Ok(material) => added.push(material),
            Err(error) => refused.push(error),
        }
    }
    (added, refused)
}

#[test]
fn the_game_leaves_terrain_ids_117_to_127_free() {
    let palette = palette();
    assert_eq!(palette.free(), (117..=127).collect::<Vec<u8>>());
    assert!(palette.holds(SAND) && !palette.holds(0) && !palette.holds(117));
    assert_eq!(palette.guid_of(SAND), Some(guid(SAND)));
    assert_eq!(palette.id_of(guid(ROCK)), Some(ROCK));
    assert_eq!(palette.guid_of(117), None, "a free id names nothing");
}

#[test]
fn materials_take_the_free_ids_in_load_order_and_the_twelfth_is_refused() {
    let requests: Vec<_> = (150..162).map(request).collect();
    let (added, refused) = add_all(&requests);
    assert_eq!(
        added.iter().map(|m| m.id).collect::<Vec<_>>(),
        (117..=127).collect::<Vec<_>>()
    );
    assert_eq!(added[0].guid, guid(150));
    assert_eq!(refused, [TerrainMaterialError::Full(11)]);
}

#[test]
fn a_guid_twice_a_material_the_game_lacks_and_a_bad_change_are_refused() {
    let mut twice = request(150);
    twice.like = ROCK;
    let mut unknown = request(151);
    unknown.fallback = 117;
    let mut weak = request(152);
    weak.changes = TerrainMaterialAdd::HEALTH;
    let mut odd = request(153);
    odd.changes = 1 << 7;
    let (added, refused) = add_all(&[request(150), twice, unknown, weak, odd]);
    assert_eq!(added.len(), 1);
    assert_eq!(
        refused,
        [
            TerrainMaterialError::Taken,
            TerrainMaterialError::NotVanilla(117),
            TerrainMaterialError::BadChanges,
            TerrainMaterialError::BadChanges,
        ]
    );
}

#[test]
fn a_material_is_named_in_the_palette_and_copies_its_rule_with_its_changes() {
    let mut tar = request(150);
    tar.changes =
        TerrainMaterialAdd::ITEM | TerrainMaterialAdd::HEALTH | TerrainMaterialAdd::FLATTENS;
    tar.item = guid(201);
    tar.health = 7;
    tar.flags = 0;
    let (added, _) = add_all(&[tar]);
    let mut world = world();
    assert_eq!(name_in(&mut world, &added), Some(()));
    let Some(Value::List(guids)) = world.field(MATERIAL_GUIDS) else {
        panic!()
    };
    assert_eq!(guids[117], Value::Guid(guid(150)));
    assert_eq!(guids[118], Value::Guid(Guid::NIL));
    let mut rules = rules();
    assert_eq!(add_rules(&mut rules, &palette().free(), &added), Some(()));
    let Some(Value::List(configs)) = rules.field(TERRAIN_CONFIGS) else {
        panic!()
    };
    assert_eq!(configs.len(), 127, "a rule for each free id");
    let new = &configs[116];
    assert_eq!(config_id(new), Some(117));
    assert_eq!(new.field(TERRAIN_ITEM), Some(&Value::Guid(guid(201))));
    assert_eq!(new.field(HEALTH_POINTS), Some(&Value::Int(7)));
    assert_eq!(new.field(CAN_BE_FLATTENED), Some(&Value::Bool(false)));
    assert_eq!(
        new.field(HARDNESS),
        Some(&Value::Name("Soft".into())),
        "a rule it does not set stays"
    );
    assert_eq!(new.field("frictionFactor"), Some(&Value::Float(0.5)));
    assert_eq!(configs[8], config(9), "the original stays");
}

#[test]
fn a_material_copies_the_effects_and_look_entries_of_the_one_it_is_like() {
    let mut rock = request(150);
    rock.like = ROCK;
    let (added, _) = add_all(&[rock]);
    let types = (0..256)
        .map(|n| Value::Uint(if n == u64::from(ROCK) { 3 } else { 0 }))
        .collect();
    let mut effects = Value::Struct(vec![(MATERIAL_TYPE.into(), Value::List(types))]);
    assert_eq!(copy_effects(&mut effects, &added), Some(()));
    let Some(Value::List(types)) = effects.field(MATERIAL_TYPE) else {
        panic!()
    };
    assert_eq!(types[117], Value::Uint(3));
    let data = (0..TERRAIN_IDS * LOOK_ENTRY)
        .map(|n| Value::Uint(n as u64 % 251))
        .collect();
    let block = Value::Struct(vec![(DATA.into(), Value::List(data))]);
    let mut blending = Value::Struct(vec![(BAKING_VOXEL_MATERIALS.into(), block)]);
    assert_eq!(copy_looks(&mut blending, &added), Some(()));
    let Some(Value::List(bytes)) = blending
        .field(BAKING_VOXEL_MATERIALS)
        .and_then(|b| b.field(DATA))
    else {
        panic!()
    };
    let like = usize::from(ROCK) * LOOK_ENTRY;
    assert_eq!(
        bytes[117 * LOOK_ENTRY..118 * LOOK_ENTRY],
        bytes[like..like + LOOK_ENTRY]
    );
    assert_eq!(
        bytes[118 * LOOK_ENTRY],
        Value::Uint(118 * LOOK_ENTRY as u64 % 251),
        "the next id stays"
    );
}

#[test]
fn a_short_look_block_is_left_as_it_is() {
    let (added, _) = add_all(&[request(150)]);
    let block = Value::Struct(vec![(DATA.into(), Value::List(vec![Value::Uint(0); 16]))]);
    let mut blending = Value::Struct(vec![(BAKING_VOXEL_MATERIALS.into(), block)]);
    assert_eq!(copy_looks(&mut blending, &added), None);
}

#[test]
fn a_removed_mods_material_takes_a_free_id_after_the_mods_as_a_copy_of_its_fallback() {
    let (added, _) = add_all(&[request(160)]);
    let recorded = [(guid(150), guid(ROCK)), (guid(160), guid(SAND))];
    let (kept, lost) = orphans(&palette(), &added, &recorded);
    assert_eq!(
        kept,
        [Material {
            owner: ModIndex::LOADER,
            guid: guid(150),
            id: 118,
            like: ROCK,
            fallback: ROCK,
            changes: Changes::default()
        }],
        "the material a mod adds now is not an orphan"
    );
    assert!(lost.is_empty());
}

#[test]
fn an_orphan_without_a_free_id_or_a_known_fallback_is_lost() {
    let (added, _) = add_all(&(150..161).map(request).collect::<Vec<_>>());
    let recorded = [(guid(170), guid(SAND))];
    assert_eq!(
        orphans(&palette(), &added, &recorded),
        (vec![], vec![guid(170)])
    );
    let unknown = [(guid(171), guid(250))];
    assert_eq!(
        orphans(&palette(), &[], &unknown),
        (vec![], vec![guid(171)])
    );
    let vanilla = [(guid(SAND), guid(ROCK))];
    assert_eq!(
        orphans(&palette(), &[], &vanilla),
        (vec![], vec![]),
        "a GUID the game names is no orphan"
    );
}

#[test]
fn a_hardness_past_unbreakable_is_refused() {
    let mut hard = request(150);
    hard.changes = TerrainMaterialAdd::HARDNESS;
    hard.hardness = 5;
    let mut harder = request(151);
    harder.changes = TerrainMaterialAdd::HARDNESS;
    harder.hardness = 6;
    let (added, refused) = add_all(&[hard, harder]);
    assert_eq!(added.len(), 1);
    assert_eq!(refused, [TerrainMaterialError::BadChanges]);
}

#[test]
fn a_guid_the_game_names_already_is_refused() {
    let (added, refused) = add_all(&[request(ROCK)]);
    assert!(added.is_empty());
    assert_eq!(refused, [TerrainMaterialError::Taken]);
}

#[test]
fn an_orphan_whose_fallback_the_game_lacks_leaves_its_free_id_to_the_next() {
    let recorded = [(guid(170), guid(250)), (guid(171), guid(SAND))];
    let (kept, lost) = orphans(&palette(), &[], &recorded);
    assert_eq!(lost, [guid(170)]);
    assert_eq!(
        kept.iter().map(|m| (m.guid, m.id)).collect::<Vec<_>>(),
        [(guid(171), 117)]
    );
}

#[test]
fn rules_without_a_material_s_like_give_it_the_first_rule_so_their_size_holds() {
    let mut rock = request(151);
    rock.like = ROCK;
    rock.changes = TerrainMaterialAdd::HEALTH;
    rock.health = 7;
    let (added, _) = add_all(&[request(150), rock]);
    let mut rules = Value::Struct(vec![(
        TERRAIN_CONFIGS.into(),
        Value::List(vec![config(u64::from(SAND))]),
    )]);
    assert_eq!(add_rules(&mut rules, &[117, 118, 119], &added), Some(()));
    let Some(Value::List(configs)) = rules.field(TERRAIN_CONFIGS) else {
        panic!()
    };
    assert_eq!(
        configs.iter().map(config_id).collect::<Vec<_>>(),
        [Some(SAND), Some(117), Some(118), Some(119)]
    );
    assert_eq!(configs[2].field(HEALTH_POINTS), Some(&Value::Int(7)));
    assert_eq!(configs[3].field(HEALTH_POINTS), Some(&Value::Int(3)));
}

/// A rule whose conditional buffs number `count`.
fn buffed(n: u64, count: usize) -> Value {
    let Value::Struct(mut fields) = config(n) else {
        unreachable!()
    };
    let buff = |at: f64| {
        Value::Struct(vec![
            (MIN_SUBMERGE.into(), Value::Float(at)),
            ("buffType".into(), Value::Guid(guid(220))),
        ])
    };
    let list = [0.25, 0.9].into_iter().take(count).map(buff).collect();
    fields.push((CONDITIONAL_BUFFS.into(), Value::List(list)));
    Value::Struct(fields)
}

#[test]
fn the_rules_come_out_one_size_whichever_materials_a_session_serves() {
    let rules = Value::Struct(vec![(
        TERRAIN_CONFIGS.into(),
        Value::List(vec![buffed(1, 0), buffed(u64::from(SAND), 2)]),
    )]);
    let mut wet = request(150);
    wet.like = SAND;
    let mut dry = request(151);
    dry.like = 1;
    let free = palette().free();
    let sizes: Vec<Vec<usize>> = [vec![], vec![wet], vec![dry, wet]]
        .iter()
        .map(|requests| {
            let mut served = rules.clone();
            let (added, _) = add_all(requests);
            add_rules(&mut served, &free, &added).unwrap();
            let Some(Value::List(configs)) = served.field(TERRAIN_CONFIGS) else {
                panic!()
            };
            configs
                .iter()
                .skip(2)
                .map(|c| match c.field(CONDITIONAL_BUFFS) {
                    Some(Value::List(list)) => list.len(),
                    _ => 0,
                })
                .collect()
        })
        .collect();
    assert!(sizes.iter().all(|s| *s == vec![2; 11]), "{sizes:?}");
    let mut served = rules;
    let (added, _) = add_all(&[dry]);
    add_rules(&mut served, &free, &added).unwrap();
    let Some(Value::List(configs)) = served.field(TERRAIN_CONFIGS) else {
        panic!()
    };
    let Some(Value::List(padded)) = configs[2].field(CONDITIONAL_BUFFS) else {
        panic!()
    };
    assert!(
        padded
            .iter()
            .all(|b| b.field(MIN_SUBMERGE) == Some(&Value::Float(f64::INFINITY))),
        "a padded buff never applies"
    );
}

#[test]
fn a_world_s_record_of_its_materials_reads_back_and_a_bad_one_reads_as_none() {
    let (added, _) = add_all(&[request(150), request(151)]);
    assert_eq!(
        held_of(&toml::to_string(&held(&added)).unwrap()),
        [guid(150), guid(151)]
    );
    assert_eq!(held_of("held = [\"no guid\"]"), []);
    assert_eq!(held_of("not toml ["), []);
}

#[test]
fn the_loaded_world_s_orphans_take_the_free_ids_first() {
    let (added, _) = add_all(&(150..159).map(request).collect::<Vec<_>>());
    let recorded = [
        (guid(170), guid(SAND)),
        (guid(171), guid(SAND)),
        (guid(172), guid(ROCK)),
    ];
    let preferred = prefer(&recorded, &[guid(172)]);
    assert_eq!(preferred[0], (guid(172), guid(ROCK)));
    let (kept, lost) = orphans(&palette(), &added, &preferred);
    assert_eq!(
        kept.iter().map(|m| (m.guid, m.id)).collect::<Vec<_>>(),
        [(guid(172), 126), (guid(170), 127)]
    );
    assert_eq!(lost, [guid(171)]);
    assert_eq!(prefer(&recorded, &[]), recorded);
}

#[test]
fn a_change_whose_rule_field_is_missing_fails_the_rules_instead_of_vanishing() {
    let mut weak = request(150);
    weak.changes = TerrainMaterialAdd::HEALTH;
    weak.health = 7;
    let (added, _) = add_all(&[weak]);
    let mut renamed = config(u64::from(SAND));
    let Value::Struct(fields) = &mut renamed else {
        panic!()
    };
    fields.retain(|(name, _)| name != HEALTH_POINTS);
    let mut rules = Value::Struct(vec![(TERRAIN_CONFIGS.into(), Value::List(vec![renamed]))]);
    assert_eq!(add_rules(&mut rules, &[117], &added), None);
}

#[test]
fn a_material_of_a_mod_turned_off_keeps_its_id_as_a_copy_of_its_fallback() {
    let mut tar = request(150);
    tar.fallback = ROCK;
    tar.changes = TerrainMaterialAdd::HEALTH;
    tar.health = 7;
    let (mut added, _) = add_all(&[tar, request(151)]);
    added[1].owner = ModIndex::new(2);
    let served = serving(&added, |owner| owner != MOD);
    assert_eq!(
        served.iter().map(|m| (m.id, m.like)).collect::<Vec<_>>(),
        [(117, ROCK), (118, SAND)],
        "the ids the mods were told hold"
    );
    assert_eq!(served[0].changes, Changes::default());
    assert_eq!(serving(&added, |_| true), added);
}

#[test]
fn changes_read_only_the_rules_the_request_sets() {
    let mut all = request(1);
    all.changes = TerrainMaterialAdd::ALL_CHANGES;
    all.item = guid(5);
    all.hardness = 2;
    all.health = 9;
    all.flags = TerrainMaterialInfo::FLATTENS;
    assert_eq!(
        Changes::of(&all),
        Some(Changes {
            item: Some(guid(5)),
            hardness: Some(2),
            health: Some(9),
            flattens: Some(true)
        })
    );
    assert_eq!(Changes::of(&request(1)), Some(Changes::default()));
}

#[test]
fn a_buff_without_its_submerge_fraction_fails_the_rules_instead_of_changing_their_size() {
    let Value::Struct(mut fields) = buffed(u64::from(SAND), 2) else {
        unreachable!()
    };
    for (name, value) in &mut fields {
        if name == CONDITIONAL_BUFFS {
            let Value::List(buffs) = value else {
                unreachable!()
            };
            for buff in buffs {
                let Value::Struct(buff) = buff else {
                    unreachable!()
                };
                buff.retain(|(field, _)| field != MIN_SUBMERGE);
            }
        }
    }
    let mut rules = Value::Struct(vec![(
        TERRAIN_CONFIGS.into(),
        Value::List(vec![buffed(1, 0), Value::Struct(fields)]),
    )]);
    assert_eq!(add_rules(&mut rules, &[117], &[]), None);
}

#[test]
fn the_server_s_save_folder_stays_as_its_configuration_names_it() {
    let json = "{\n\t\"saveDirectory\": \"./saves\"\n}";
    assert_eq!(saves_folder(Some(json)), std::path::Path::new("./saves"));
    assert_eq!(saves_folder(Some("{}")), std::path::Path::new("./savegame"));
    assert_eq!(saves_folder(None), std::path::Path::new("./savegame"));
}

#[test]
fn only_free_ids_no_served_material_takes_are_padding() {
    let (added, _) = add_all(&[request(150)]);
    let free = palette().free();
    let taken = added[0].id;
    assert!(!pads(&free, &added, taken));
    assert!(pads(&free, &[], taken));
    assert!(!pads(&free, &added, SAND));
    assert_eq!(
        free.iter().filter(|&&id| pads(&free, &added, id)).count(),
        free.len() - 1
    );
}

#[test]
fn only_mods_off_for_the_game_placed_before_the_shifted_one_moved_its_id() {
    let owner = ModIndex::new;
    let base = add_all(&[request(150)]).0[0];
    let at = |n, id| Material {
        owner: owner(n),
        id,
        ..base
    };
    // Placed in load order: 2 and 3 before 4, then 5 after it.
    let added = [at(2, 117), at(3, 118), at(3, 119), at(4, 120), at(5, 121)];
    let off = |m: ModIndex| m != owner(4) && m != owner(3);
    assert_eq!(moved_by(&added, owner(4), off), [owner(2)]);
    assert_eq!(moved_by(&added, owner(4), |_| true), [owner(2), owner(3)]);
    assert_eq!(moved_by(&added, owner(2), |_| true), []);
}

#[test]
fn a_session_rebuilds_only_for_another_table_once_one_was_built() {
    let (one, _) = add_all(&[request(150)]);
    let (two, _) = add_all(&[request(150), request(151)]);
    assert!(!Session::Unplaced.rebuilds_for(&one));
    assert!(!Session::Own(one.clone()).rebuilds_for(&one));
    assert!(!Session::Host(one.clone()).rebuilds_for(&one));
    assert!(Session::Own(one.clone()).rebuilds_for(&two));
    assert!(Session::Host(two).rebuilds_for(&one));
}

#[test]
fn a_host_table_that_does_not_take_is_told_naming_who_hosts() {
    assert!(untaken_text(Host::Server).contains("server's terrain ids"));
    assert!(untaken_text(Host::Player).contains("host's terrain ids"));
    let uk = Some(enshwick_core::i18n::Language::UK_UA);
    assert!(
        crate::engine::texts::for_reader(uk, || untaken_text(Host::Server))
            .contains("ґрунту сервера")
    );
}

#[test]
fn a_moved_terrain_id_is_told_in_the_reader_s_language_naming_who_hosts() {
    use crate::engine::texts::for_reader;
    use enshwick_core::i18n::Language;
    let extra = ["Caves".to_owned(), "Dunes".to_owned()];
    assert_eq!(
        moved_text(Host::Server, "Ores", &[]),
        "Ores gives its terrain another id here than the server does. Run the server's mods in its versions."
    );
    assert_eq!(
        moved_text(Host::Player, "Ores", &extra),
        "These Enshwick mods add terrain, and the host does not run them:\n\nCaves\nDunes\n\nSo Ores gives its terrain another id here than the host does. Turn them off on the Mods screen and restart the game."
    );
    let uk = Some(Language::UK_UA);
    assert_eq!(
        for_reader(uk, || moved_text(Host::Server, "Ores", &extra)),
        "Ці моди Enshwick додають ґрунт, а сервер їх не запускає:\n\nCaves\nDunes\n\nТож Ores дає своєму ґрунту тут інший номер, ніж сервер. Вимкніть їх на екрані модів і перезапустіть гру."
    );
    assert_eq!(
        for_reader(uk, || moved_text(Host::Player, "Ores", &[])),
        "Ores дає своєму ґрунту тут інший номер, ніж хост. Запустіть моди хоста в його версіях."
    );
}

#[test]
fn a_host_s_table_goes_once_when_the_game_played_with_it_ends() {
    let played = Played::new();
    assert!(!played.changed(true, || false), "a game the client hosts");
    assert!(!played.changed(false, || unreachable!()), "its end");
    assert!(!played.changed(true, || true), "a joined game starts");
    assert!(played.changed(false, || unreachable!()), "and ends");
    assert!(
        !played.changed(false, || unreachable!()),
        "only once per game"
    );
}

#[test]
fn a_host_s_table_taken_while_still_in_a_game_stays_when_that_game_ends() {
    let played = Played::new();
    assert!(!played.changed(true, || true), "a joined game starts");
    played.switched();
    assert!(
        !played.changed(false, || unreachable!()),
        "the next host's table, taken in play, stays"
    );
}

#[test]
fn an_empty_table_changes_no_palette_world_effect_or_look() {
    let types = (0..256).map(|n| Value::Uint(n % 7)).collect();
    let effects = Value::Struct(vec![(MATERIAL_TYPE.into(), Value::List(types))]);
    let data = (0..TERRAIN_IDS * LOOK_ENTRY)
        .map(|n| Value::Uint(n as u64 % 251))
        .collect();
    let block = Value::Struct(vec![(DATA.into(), Value::List(data))]);
    let blending = Value::Struct(vec![(BAKING_VOXEL_MATERIALS.into(), block)]);
    let unchanged = |value: &Value, edit: fn(&mut Value, &[Material]) -> Option<()>| {
        let mut edited = value.clone();
        edit(&mut edited, &[]).is_some() && edited == *value
    };
    assert!(unchanged(&world(), name_in));
    assert!(unchanged(&effects, copy_effects));
    assert!(unchanged(&blending, copy_looks));
}
