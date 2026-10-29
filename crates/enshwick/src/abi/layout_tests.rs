//! The layout of every ABI type, held against the committed golden file
//! `layout.txt`, whose hash is [`ABI_VERSION`]: any change to the layout is
//! a new ABI version.
//!
//! After an intended change, run the tests with `ENSHWICK_ABI_LAYOUT=update`
//! to rewrite the file.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use super::*;

/// A type's size, alignment and fields in declaration order.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Shape {
    size: usize,
    align: usize,
    fields: Vec<Field>,
}

/// One field of a [`Shape`]: where it sits and what it holds, so a field
/// that keeps its size but changes its type, such as a function pointer with
/// another signature, still changes the layout.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Field {
    name: String,
    offset: usize,
    size: usize,
    ty: String,
}

/// The layout of the whole ABI at one version.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Snapshot {
    types: BTreeMap<String, Shape>,
}

/// The field `name` at `offset`, of the type the accessor `_field` picks out
/// of an `S`.
fn field<S, T>(name: &str, offset: usize, _field: fn(&S) -> &T) -> Field {
    Field {
        name: name.to_owned(),
        offset,
        size: size_of::<T>(),
        ty: type_text(core::any::type_name::<T>()),
    }
}

/// `name`, a type as `core::any::type_name` writes it, with every path cut
/// to its last segment: `core::option::Option<enshwick::abi::Str>` becomes
/// `Option<Str>`. Moving a type to another module leaves the layout alone;
/// its own entry holds what it is.
fn type_text(name: &str) -> String {
    let mut text = String::with_capacity(name.len());
    let mut rest = name;
    while let Some(c) = rest.chars().next() {
        if let Some(after) = rest.strip_prefix("::") {
            let kept = text.trim_end_matches(|c: char| c.is_alphanumeric() || c == '_');
            text.truncate(kept.len());
            rest = after;
        } else {
            text.push(c);
            rest = &rest[c.len_utf8()..];
        }
    }
    text
}

/// The [`Shape`] of `$ty` with the fields named.
macro_rules! shape {
    ($ty:ident { $($field:tt),* }) => {
        Shape {
            size: size_of::<$ty>(),
            align: align_of::<$ty>(),
            fields: vec![$(field(
                stringify!($field),
                core::mem::offset_of!($ty, $field),
                |value: &$ty| &value.$field,
            )),*],
        }
    };
}

/// Every ABI type with all of its fields; the test
/// `every_abi_type_and_field_is_listed` holds this list to the source.
macro_rules! shapes {
    ($($ty:ident { $($field:tt),* }),* $(,)?) => {
        fn current() -> Snapshot {
            let mut types = BTreeMap::new();
            $(types.insert(stringify!($ty).to_owned(), shape!($ty { $($field),* }));)*
            Snapshot { types }
        }

        const LISTED: &[(&str, &[&str])] = &[$((stringify!($ty), &[$(stringify!($field)),*])),*];
    };
}

shapes! {
        Header { size, abi_version },
        Str { ptr, len },
        Status { 0 },
        Side { 0 },
        Level { 0 },
        Event { 0 },
        Entity { 0 },
        ItemId { 0 },
        PlayerKey { 0 },
        RecipeId { 0 },
        SkillNode { 0 },
        KnowledgeId { 0 },
        BuffTypeId { 0 },
        NpcId { 0 },
        PlayerDeathData { header, entity, cause, key, position, cause_guid, was_deadly_fog, drowned, reserved, killer_entity, reserved_tail },
        KnowledgeData { header, entity, id, value },
        QuestData { header, entity, quest },
        KnowledgeInput { header, player, id, kind, scope, reserved, value, refusal, allowed },
        ChunkData { header, index, x, z, size },
        BuildingData { header, entity, player, item, kind, replay, template, position },
        PlayerInfo { header, slot, entity, key, permissions, name_len, name },
        TickData { header, step, elapsed_steps, elapsed_nanos, step_nanos, cost_nanos, game },
        TickHandle { 0 },
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
        ItemData { header, id, max_stack, category, rarity, flags, placed_entity },
        Bounds { min, max },
        AltarInfo { header, entity, level, tilted, level_count, reserved, zone, levels, position },
        AltarLimit { header, flame_level, vanilla, allowed, reserved },
        BaseLimitInfo { header, zones, bases, vanilla_zones, reserved, vanilla_bases, max_zones, max_bases },
        AltarZone { header, altar, level, reserved, grow, height },
        BuildingZoneCheck { header, zones, zone_count, object, margin },
        SurvivorLookup { header, survivor, vanilla, purpose, entity, cell, bed_of, token },
        BedCheck { header, npc_id, entity, bed_id, home, bed_at },
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
        CraftCheck { header, crafting_operator_id, crafting_station_id, recipe_id, recipe_amount, reserved },
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
        Voxel { kind, material, fill, reserved },
        BuildRights { header, player, refusal, min, max },
        TransactionOp { kind, reserved, data },
        TransactionInfo { header, state, reason, ops, applied, bricks, refusal, reserved, holds, created },
        PropPlace { header, item, reserved, position, orientation },
        PropRemove { header, entity, flags },
        VoxelWrite { header, min, size, voxels, count },
        VoxelRegion { header, min, size },
        BlockPlace { header, shape, material, position, rotation },
        UiFrame { header, token, screen, reserved },
        UiWidget { header, kind, flags, style, steps, text, detail, options, option_count, index, value, min, max, result, anchor, reserved },
        UiMarker { header, position, label, color, marker_type },
        UiDialog { header, title, message, detail, buttons, flags },
        FogInfo { header, width, height, metres_per_pixel, size_z, version, session },
        MapMarkerInfo { header, position, marker_type, label },
        MapFrameInfo { header, origin, size, metres_per_cell },
        MapMarkerTypeInfo { header, type_name, marker_id, sorting_category, is_fast_travel_destination, is_custom_marker_type, show_above_fog_of_war, show_label, reserved },
        MarkerTypeSorting { 0 },
        HostApi { header, host_version, build, symbol, system, field_offset, type_size, hook, caller_frame, fault, log, settings, publish, service, constant, data_dir, log_level, subscribe, type_hash, component, entity_template, player, players, serve_resource, add_text, mods, sync, code_range, patch_code, workshop_recipes, hosting, server_world, chat, set_label, rename_filter, join_missing, hook_at, inventory_open, item_info, inventory_submit, status, run_console, altars_open, altars, building_zone_check_filter, survivor_lookup, survivor_bed_check, summon_filter, summon_item, survivor_status, interaction_filter, skill_override, comfort_filter, experience_filter, raise_base_limit, base_zones, game_event, entities, destroy, save_world, world_key, component_write, emit, teleport, spawn, craft_filter, build_filter, net_channel, net_send, net_ready, notify, time_of_day, set_time_of_day, time_speed_control, set_time_speed, game_settings, set_game_settings, damage_filter, buff_types, buffs, apply_buff, remove_buff, knowledge, grant_knowledge, quests, weather_at, weather_fronts, set_weather, terrain_materials, terrain_edit, config_schema, summoned, item_data_open, item_data_get, item_data_set, ui_frame, ui_widget, ui_dialog, ui_hud, ui_markers, config_check, texts, content_serve, content_read, icons_ready, stat_open, stat_info, stat_value, stat_modify, stat_buff, stats_changed, stat_sources, stat_watch, define_event, subscribe_event, send_event, affix_open, affix_roll, item_lines, affix_info, migrate, declare_content, entity_data_open, replaced, knowledge_input_filter, language, keep_game_settings, forget_game_settings, revoke_knowledge, account_knowledge, resource_read, game_text, rebuild_resource, recipe_rule, recipe_disabled, define_point, attach_point, run_point, actions, event_types, recipe_kind, feature, rate, set_rate, change_data, loop_wait, server_wait, task_workers, altar_zone_filter, altar_zones_changed, altar_limit_filter, base_limit, voxel_read, declare_change, set_change, build_rights, transaction_commit, transaction_undo, transaction_status, transaction_commit_for, world_quiet, fog_open, fog_read, serve_call, find_call, call, transaction_entities, world_persists, map_markers, account_action, allowlist_set, allowlist_has, subscribe_tick, set_tick_every, map_frame, request_base_limits, map_marker_types },
        AccountActionType { 0 },
        LoopWaitData { header, ms, reserved, left_ns },
        LoopWaitSites { 0 },
        EventTypeInfo { header, name, hash, size },
        RateId { 0 },
        RecipeRuleKind { 0 },
        RecipeKind { 0 },
        DataField { 0 },
        ChangeOp { 0 },
        DataChange { header, entry, field, op, value, source, item, select, categories },
        DataSelect { 0 },
        ChangeId { 0 },
        EntityDataFlags { 0 },
        MigrationStep { kind, from, to, from_guid, to_guid },
        Migration { id, steps, count },
        MigrationSet { header, migrations, count, map, user },
        LayoutField { name, hash },
        Layout { fields, count, size },
        ServiceSpec { header, name, major, layout, table },
        ServiceFound { header, table, fields, owner, owner_version, reason },
        EventSpec { header, name, layout },
        CallSpec { header, name, major, input, output },
        PointMode { 0 },
        PointFlags { 0 },
        PointSpec { header, name, mode, flags, input, output },
        PointLink { header, point, before, after, before_count, after_count, priority },
        PointHandle { header, id, live },
        PointVerdict { 0 },
}

impl Snapshot {
    fn render(&self) -> String {
        let mut text = String::new();
        for (name, shape) in &self.types {
            let _ = writeln!(text, "{name} size {} align {}", shape.size, shape.align);
            for Field {
                name,
                offset,
                size,
                ty,
            } in &shape.fields
            {
                let _ = writeln!(text, "    {name} at {offset} size {size} type {ty}");
            }
        }
        text
    }

    fn parse(text: &str) -> Option<Self> {
        let mut layout = Self {
            types: BTreeMap::new(),
        };
        let mut last = None;
        for line in text.lines() {
            let words: Vec<&str> = line.split_whitespace().collect();
            if let Some(field) = line.strip_prefix("    ") {
                let (place, ty) = field.split_once(" type ")?;
                let words: Vec<&str> = place.split_whitespace().collect();
                let [name, "at", offset, "size", size] = words[..] else {
                    return None;
                };
                let shape = layout.types.get_mut(last.as_deref()?)?;
                shape.fields.push(Field {
                    name: name.to_owned(),
                    offset: offset.parse().ok()?,
                    size: size.parse().ok()?,
                    ty: ty.to_owned(),
                });
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

fn golden_path() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src/abi/layout.txt")
}

#[test]
fn the_abi_layout_matches_the_golden_file() {
    let new = current();
    let path = golden_path();
    let text = std::fs::read_to_string(&path).unwrap_or_default();
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

#[test]
fn a_rendered_layout_reads_back() {
    let layout = current();
    assert_eq!(Snapshot::parse(&layout.render()), Some(layout));
}

#[test]
fn the_abi_version_is_the_hash_of_the_golden_file() {
    let text = std::fs::read(golden_path()).unwrap();
    assert_eq!(layout_version(&text), ABI_VERSION);
}

#[test]
fn every_change_to_the_layout_is_another_abi_version() {
    let text = current().render();
    let moved = text.replacen(" at 8 ", " at 16 ", 1);
    let renamed = text.replacen("header at", "head at", 1);
    let grown = format!("{text}Extra size 8 align 8\n    header at 0 size 8\n");
    for changed in [moved, renamed, grown] {
        assert_ne!(changed, text);
        assert_ne!(
            layout_version(changed.as_bytes()),
            layout_version(text.as_bytes())
        );
    }
}

/// Two versions of one struct whose function pointer keeps its size and
/// changes its signature.
mod fixture {
    #[repr(C)]
    pub(super) struct Before {
        pub(super) call: extern "system" fn(u32) -> u32,
    }

    #[repr(C)]
    pub(super) struct After {
        pub(super) call: extern "system" fn(u64, u32) -> u32,
    }
}

/// The ABI version of a layout holding only `shape`, under one name.
fn version_of(shape: Shape) -> u32 {
    let types = BTreeMap::from([("Fixture".to_owned(), shape)]);
    layout_version(Snapshot { types }.render().as_bytes())
}

#[test]
fn a_changed_function_signature_is_another_abi_version() {
    use fixture::{After, Before};
    assert_eq!(size_of::<Before>(), size_of::<After>());
    assert_ne!(
        version_of(shape!(Before { call })),
        version_of(shape!(After { call }))
    );
}

#[test]
fn a_type_is_written_without_its_paths() {
    assert_eq!(
        type_text(
            "core::option::Option<unsafe extern \"system\" fn(*const enshwick::abi::Str, u32) -> enshwick::abi::Status>"
        ),
        "Option<unsafe extern \"system\" fn(*const Str, u32) -> Status>"
    );
}

#[test]
fn line_endings_leave_the_abi_version_alone() {
    let text = current().render();
    let windows = text.replace('\n', "\r\n");
    assert_eq!(
        layout_version(windows.as_bytes()),
        layout_version(text.as_bytes())
    );
}

#[test]
fn the_c_header_defines_this_abi_version() {
    let header =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("include/enshwick.h"))
            .unwrap();
    let define = format!("#define ENSHWICK_ABI_VERSION {ABI_VERSION:#010x}");
    assert!(
        header.lines().any(|line| line == define),
        "include/enshwick.h lacks `{define}`: run `cargo xtask header`"
    );
}
