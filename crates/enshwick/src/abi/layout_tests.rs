//! The layout of every ABI type, held against the committed golden file
//! `layout.txt`: a change passes only as an append with a minor bump, or with
//! a major bump.
//!
//! After an intended change, run the tests with `ENSHWICK_ABI_LAYOUT=update`
//! to rewrite the file; the rule above still holds while it is rewritten.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use super::{ABI_MAJOR, ABI_MINOR, *};

/// A type's size, alignment and fields as `(name, offset, size)` in
/// declaration order.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Shape {
    size: usize,
    align: usize,
    fields: Vec<(String, usize, usize)>,
}

/// The layout of the whole ABI at one version.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Snapshot {
    major: u16,
    minor: u16,
    types: BTreeMap<String, Shape>,
}

/// The size of the field `field` picks out of an `S`.
fn field_size<S, T>(_field: fn(&S) -> &T) -> usize {
    size_of::<T>()
}

/// Every ABI type with all of its fields; the test
/// `every_abi_type_and_field_is_listed` holds this list to the source.
macro_rules! shapes {
    ($($ty:ident { $($field:tt),* }),* $(,)?) => {
        fn current() -> Snapshot {
            let mut types = BTreeMap::new();
            $(
                types.insert(
                    stringify!($ty).to_owned(),
                    Shape {
                        size: size_of::<$ty>(),
                        align: align_of::<$ty>(),
                        fields: vec![$((
                            stringify!($field).to_owned(),
                            core::mem::offset_of!($ty, $field),
                            field_size(|value: &$ty| &value.$field),
                        )),*],
                    },
                );
            )*
            Snapshot { major: ABI_MAJOR, minor: ABI_MINOR, types }
        }

        const LISTED: &[(&str, &[&str])] = &[$((stringify!($ty), &[$(stringify!($field)),*])),*];
    };
}

shapes! {
        Header { size, abi_major, abi_minor },
        Str { ptr, len },
        Status { 0 },
        Side { 0 },
        Level { 0 },
        Event { 0 },
        PlayerDeathData { header, entity, cause, key, position, killer, deadly_shroud, drowned, reserved, killer_entity, reserved_tail },
        KnowledgeData { header, entity, id, value },
        QuestData { header, entity, quest },
        KnowledgeInput { header, player, id, kind, scope, reserved, value, refusal, allowed },
        ChunkData { header, index, x, z, size },
        BuildingData { header, entity, player, item, kind, replay, template, position },
        PlayerInfo { header, slot, entity, key, permissions, name_len, name },
        TickData { header, step },
        PlayerData { header, slot, entity, key },
        ActionCall { header, kind, language, player, action, args },
        RenameCall { header, entity, known, player, text },
        ResourceKey { guid, type_hash, part },
        Fingerprint { guid, age },
        BuildInfo { header, side, base, fingerprint, table },
        BuildTable { 0 },
        Symbol { name, rva },
        BuildRow { side, fingerprint, symbols, symbol_count, constants, constant_count },
        Relation { 0 },
        Dependency { id, range, relation },
        ManifestFlags { 0 },
        Manifest { header, id, version, side, flags, dependencies, dependency_count, builds, build_count, name, description, authors, author_count, license, homepage, points },
        HookFlags { 0 },
        HookSpec { target, detour, next, priority, flags },
        CallerFrame { rsp, rbx, rbp, rdi, rsi, r12, r13, r14, r15 },
        ModInfo { header, id, version, side, flags, off, file, name, description, authors, author_count, license, homepage, replaced_by },
        ModHandle { 0 },
        Registers { rax, rcx, rdx, rbx, rsp, rbp, rsi, rdi, r8, r9, r10, r11, r12, r13, r14, r15, rflags },
        MidHookSpec { address, handler, user, priority, flags },
        InventoryOp { kind, flags, target, other, item, count },
        InventoryRequest { header, flags, actor, ops, op_count, reserved },
        InventoryOpResult { error, remaining },
        InventoryResult { header, outcome, op_count, ops },
        ItemData { header, id, max_stack, category, rarity, flags },
        Bounds { min, max },
        AltarInfo { header, entity, level, tilted, level_count, reserved, zone, levels, position },
        AltarZoneSize { altar, level, reserved, grow, height },
        ZoneCheck { header, zones, zone_count, object, margin },
        SurvivorLookup { header, survivor, vanilla, purpose, entity, cell, bed_of, token },
        BedCheck { header, survivor, entity, bed, home, bed_at },
        Summon { header, item, ammo, template, target, slot, pide, meter, consumed, reserved },
        Interaction { header, kind, player, target, item, known, reserved, position },
        SkillCheck { header, node, gate, unlocked, reserved },
        Comfort { header, category, value, position },
        ExperienceRow { entity, level, progress, need, gained },
        ExperiencePass { header, rows, row_count, level_cap, need_for, token },
        GameEventData { header, type_hash, size, data },
        EntityPlace { entity, reserved, position },
        TeleportRequest { header, entity, flags, position, orientation },
        SpawnRequest { header, template, position, orientation, level, reserved, lifetime_ns },
        CraftCheck { header, player, station, recipe, amount },
        BuildCheck { header, player, item, position },
        NetMessage { header, channel, reserved, peer, data, size, reserved_tail },
        DaytimeInfo { header, time, day_start, day_end, day_length, night_length, speed, flags },
        DamageCheck { header, cause, target, source, attacker, amounts, total },
        StatDefData { header, ty, format, reserved, default, min, max, text },
        StatRef { index, key },
        StatModifierData { stat, op, reserved, value },
        StatModify { header, entity, key, modifier, flags, duration_ms },
        StatChange { header, entity, stat, old, new, reserved },
        StatSource { kind, op, reserved, stat, value, id, owner },
        AffixDefData { header, text, group, modifiers, modifier_count, weight, categories, rarity, reserved },
        AffixModifierData { stat, op, reserved, min, max, step },
        AffixRoll { header, pide, item, min, max, max_rarity, flags, seed },
        ItemLine { modifier, affix },
        BuffTypeInfo { header, id, slot, flags, name, duration },
        ActiveBuff { header, id, entity, slot, level, duration, remaining },
        BuffRequest { header, entity, id, level, reserved, duration },
        KnowledgeInfo { header, id, kind, scope, reserved, value, flags },
        WeatherFront { header, layer, effect, position, radius, reserved, remaining },
        TerrainMaterialInfo { header, id, flags, hardness, reserved, item, health },
        TerrainEdit { header, kind, shape, flags, material, position, radius, height, protect },
        UiFrame { header, token, screen, reserved },
        UiWidget { header, kind, flags, style, steps, text, detail, options, option_count, index, value, min, max, result, anchor, reserved },
        UiMarker { header, position, label, color, reserved },
        UiDialog { header, title, message, detail, buttons, flags },
        HostApi { header, host_version, build, symbol, system, field_offset, type_size, hook, caller_frame, fault, log, settings, publish, service, constant, data_dir, log_level, subscribe, type_hash, component, entity_template, player, players, serve_resource, add_text, mods, sync, code_range, patch_code, workshop_recipes, hosting, server_world, retired_command, chat, set_label, rename_filter, join_missing, hook_at, inventory_open, item_info, inventory_submit, status, run_console, altars_open, altars, altar_zones_control, set_altar_zones, zone_check_filter, survivor_lookup, survivor_bed_check, summon_filter, summon_item, survivor_status, interaction_filter, skill_override, comfort_filter, experience_filter, altar_limit, raise_base_limit, base_zones, game_event, entities, destroy, save_world, world_key, component_write, emit, teleport, spawn, craft_filter, build_filter, net_channel, net_send, net_ready, notify, time_of_day, set_time_of_day, time_speed_control, set_time_speed, game_settings, set_game_settings, damage_filter, buff_types, buffs, apply_buff, remove_buff, knowledge, grant_knowledge, quests, weather_at, weather_fronts, set_weather, terrain_materials, terrain_edit, config_schema, summoned, item_data_open, item_data_get, item_data_set, ui_frame, ui_widget, ui_dialog, ui_hud, ui_markers, config_check, texts, content_serve, content_read, icons_ready, stat_open, stat_info, stat_value, stat_modify, stat_buff, stats_changed, stat_sources, stat_watch, define_event, subscribe_event, send_event, affix_open, affix_roll, item_lines, affix_info, migrate, declare_content, entity_data_open, replaced, knowledge_input_filter, language, keep_game_settings, forget_game_settings, revoke_knowledge, account_knowledge, resource_read, game_text, rebuild_resource, recipe_rule, recipe_disabled, define_point, attach_point, run_point, actions, rate, set_rate },
        RateId { 0 },
        RecipeRuleKind { 0 },
        EntityDataFlags { 0 },
        MigrationStep { kind, from, to, from_guid, to_guid },
        Migration { id, steps, count },
        MigrationSet { header, migrations, count, map, user },
        LayoutField { name, hash },
        Layout { fields, count, size },
        ServiceSpec { header, name, major, layout, table },
        ServiceFound { header, table, fields, owner, owner_version, reason },
        EventSpec { header, name, layout },
        PointMode { 0 },
        PointFlags { 0 },
        PointSpec { header, name, mode, flags, input, output },
        PointLink { header, point, before, after, before_count, after_count, priority },
        PointHandle { header, id, live },
        PointVerdict { 0 },
}

impl Snapshot {
    fn render(&self) -> String {
        let mut text = format!("abi {}.{}\n", self.major, self.minor);
        for (name, shape) in &self.types {
            let _ = writeln!(text, "{name} size {} align {}", shape.size, shape.align);
            for (field, offset, size) in &shape.fields {
                let _ = writeln!(text, "    {field} at {offset} size {size}");
            }
        }
        text
    }

    fn parse(text: &str) -> Option<Self> {
        let mut lines = text.lines();
        let (major, minor) = lines.next()?.strip_prefix("abi ")?.split_once('.')?;
        let mut layout = Self {
            major: major.parse().ok()?,
            minor: minor.parse().ok()?,
            types: BTreeMap::new(),
        };
        let mut last = None;
        for line in lines {
            let words: Vec<&str> = line.split_whitespace().collect();
            if let Some(field) = line.strip_prefix("    ") {
                let words: Vec<&str> = field.split_whitespace().collect();
                let [name, "at", offset, "size", size] = words[..] else {
                    return None;
                };
                let shape = layout.types.get_mut(last.as_deref()?)?;
                shape
                    .fields
                    .push((name.to_owned(), offset.parse().ok()?, size.parse().ok()?));
            } else {
                let [name, "size", size, "align", align] = words[..] else {
                    return None;
                };
                let shape = Shape {
                    size: size.parse().ok()?,
                    align: align.parse().ok()?,
                    fields: Vec::new(),
                };
                layout.types.insert(name.to_owned(), shape);
                last = Some(name.to_owned());
            }
        }
        Some(layout)
    }
}

/// How a layout changed from the one before it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Change {
    /// Nothing moved, grew or went away.
    None,
    /// New types, or fields appended past the end of a type's old fields.
    Append,
    /// A type or field went away, moved, shrank, or changed its alignment.
    Break,
}

fn change(old: &Snapshot, new: &Snapshot) -> Change {
    let mut change = Change::None;
    for (name, before) in &old.types {
        let Some(after) = new.types.get(name) else {
            return Change::Break;
        };
        if after.align != before.align
            || after.size < before.size
            || after.fields.len() < before.fields.len()
            || after.fields[..before.fields.len()] != before.fields[..]
        {
            return Change::Break;
        }
        if after != before {
            change = Change::Append;
        }
    }
    if new.types.len() > old.types.len() {
        change = Change::Append;
    }
    change
}

/// Why `new` may not replace `old`, or `None` when its versions allow it.
fn refusal(old: &Snapshot, new: &Snapshot) -> Option<String> {
    match change(old, new) {
        Change::Break if new.major <= old.major => Some(format!(
            "the layout changed in a way older mods cannot read: bump ABI_MAJOR past {}",
            old.major
        )),
        Change::Append if new.major == old.major && new.minor <= old.minor => Some(format!(
            "the layout grew: bump ABI_MINOR past {}",
            old.minor
        )),
        _ => None,
    }
}

fn golden_path() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src/abi/layout.txt")
}

#[test]
fn the_abi_layout_matches_the_golden_file() {
    let new = current();
    let path = golden_path();
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let old = Snapshot::parse(&text);
    if let Some(why) = old.as_ref().and_then(|old| refusal(old, &new)) {
        panic!("{why}");
    }
    let rendered = new.render();
    if std::env::var_os("ENSHWICK_ABI_LAYOUT").is_some_and(|value| value == "update") {
        std::fs::write(&path, rendered).unwrap();
        return;
    }
    assert!(
        text == rendered,
        "the ABI layout differs from {}: run the tests with ENSHWICK_ABI_LAYOUT=update and commit the file",
        path.display()
    );
}

/// The structs of the ABI source, with their field names in order.
fn structs_in_source() -> BTreeMap<String, Vec<String>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = vec![root.join("abi.rs")];
    if let Ok(dir) = std::fs::read_dir(root.join("abi")) {
        files.extend(dir.filter_map(|entry| entry.ok().map(|entry| entry.path())));
    }
    let mut found = BTreeMap::new();
    for file in files
        .iter()
        .filter(|file| file.extension().is_some_and(|ext| ext == "rs"))
    {
        let source = std::fs::read_to_string(file).unwrap();
        let parsed = syn::parse_file(&source).unwrap();
        for item in parsed.items {
            let syn::Item::Struct(item) = item else {
                continue;
            };
            let has_repr = item.attrs.iter().any(|attr| attr.path().is_ident("repr"));
            if !has_repr {
                continue;
            }
            let fields = item
                .fields
                .iter()
                .enumerate()
                .map(|(index, field)| {
                    field
                        .ident
                        .as_ref()
                        .map_or_else(|| index.to_string(), ToString::to_string)
                })
                .collect();
            found.insert(item.ident.to_string(), fields);
        }
    }
    found
}

#[test]
fn every_abi_type_and_field_is_listed() {
    let listed: BTreeMap<String, Vec<String>> = LISTED
        .iter()
        .map(|(name, fields)| {
            (
                (*name).to_owned(),
                fields.iter().map(|f| (*f).to_owned()).collect(),
            )
        })
        .collect();
    assert_eq!(listed, structs_in_source());
}

/// A field as `(name, offset, size)`.
type FieldRow<'a> = (&'a str, usize, usize);

fn layout(major: u16, minor: u16, types: &[(&str, usize, &[FieldRow<'_>])]) -> Snapshot {
    Snapshot {
        major,
        minor,
        types: types
            .iter()
            .map(|(name, size, fields)| {
                let shape = Shape {
                    size: *size,
                    align: 8,
                    fields: fields
                        .iter()
                        .map(|(f, at, size)| ((*f).to_owned(), *at, *size))
                        .collect(),
                };
                ((*name).to_owned(), shape)
            })
            .collect(),
    }
}

#[test]
fn an_appended_field_needs_a_minor_bump() {
    let old = layout(2, 5, &[("Call", 16, &[("header", 0, 8), ("a", 8, 8)])]);
    let grown = &[(
        "Call",
        24,
        &[("header", 0, 8), ("a", 8, 8), ("b", 16, 8)][..],
    )];
    assert_eq!(change(&old, &layout(2, 5, grown)), Change::Append);
    assert!(refusal(&old, &layout(2, 5, grown)).is_some());
    assert!(refusal(&old, &layout(2, 6, grown)).is_none());
}

#[test]
fn a_moved_field_needs_a_major_bump() {
    let old = layout(2, 5, &[("Call", 16, &[("header", 0, 8), ("a", 8, 8)])]);
    let moved = &[(
        "Call",
        24,
        &[("header", 0, 8), ("b", 8, 8), ("a", 16, 8)][..],
    )];
    assert_eq!(change(&old, &layout(2, 9, moved)), Change::Break);
    assert!(refusal(&old, &layout(2, 9, moved)).is_some());
    assert!(refusal(&old, &layout(3, 0, moved)).is_none());
}

#[test]
fn a_removed_type_breaks_and_a_new_one_appends() {
    let old = layout(2, 5, &[("Call", 8, &[("header", 0, 8)])]);
    assert_eq!(change(&old, &layout(2, 5, &[])), Change::Break);
    let more = &[
        ("Call", 8, &[("header", 0, 8)][..]),
        ("Data", 8, &[("header", 0, 8)][..]),
    ];
    assert_eq!(change(&old, &layout(2, 6, more)), Change::Append);
}

#[test]
fn a_rendered_layout_reads_back() {
    let layout = current();
    assert_eq!(Snapshot::parse(&layout.render()), Some(layout));
}
