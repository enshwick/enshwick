//! The layout of every ABI type, held against the committed golden file
//! `layout.txt`, whose first line is the [`ABI_MAJOR`] and [`ABI_MINOR`] it
//! was written at. A change of the layout needs the next minor when it only
//! appends, else a new major.
//!
//! After an intended change, set the version the failing test names and run
//! the tests with `ENSHWICK_ABI_LAYOUT=update` to rewrite the file.

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

/// The layout of the whole ABI at one version, with the
/// [`Headed::BORN`](crate::abi::Headed::BORN) of each headed type.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Snapshot {
    types: BTreeMap<String, Shape>,
    born: BTreeMap<String, usize>,
}

/// A type the macro `shapes!` asks for its [`Headed::BORN`](crate::abi::Headed::BORN): through
/// [`BornOfHeaded`] for a headed type, else through [`BornOfOther`], chosen
/// by autoref on `&&Probe`.
struct Probe<T>(core::marker::PhantomData<T>);

trait BornOfHeaded {
    fn born(&self) -> Option<usize>;
}

impl<T: crate::abi::Headed> BornOfHeaded for &Probe<T> {
    fn born(&self) -> Option<usize> {
        Some(T::BORN)
    }
}

trait BornOfOther {
    fn born(&self) -> Option<usize>;
}

impl<T> BornOfOther for Probe<T> {
    fn born(&self) -> Option<usize> {
        None
    }
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
            let mut born = BTreeMap::new();
            $(
                types.insert(stringify!($ty).to_owned(), shape!($ty { $($field),* }));
                if let Some(end) = (&&Probe::<$ty>(core::marker::PhantomData)).born() {
                    born.insert(stringify!($ty).to_owned(), end);
                }
            )*
            Snapshot { types, born }
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
        Manifest { header, id, version, side, flags, dependencies, dependency_count, builds, build_count, name, description, authors, author_count, license, homepage, points, exports, export_count },
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
        HostApi { header, host_version, build, symbol, system, field_offset, type_size, hook, caller_frame, fault, log, settings, publish, service, constant, data_dir, log_level, subscribe, type_hash, component, entity_template, player, players, serve_resource, add_text, mods, sync, code_range, patch_code, workshop_recipes, hosting, server_world, chat, set_label, rename_filter, join_missing, hook_at, inventory_open, item_info, inventory_submit, status, run_console, altars_open, altars, building_zone_check_filter, survivor_lookup, survivor_bed_check, summon_filter, summon_item, survivor_status, interaction_filter, skill_override, comfort_filter, experience_filter, raise_base_limit, base_zones, game_event, entities, destroy, save_world, world_key, component_write, emit, teleport, spawn, craft_filter, build_filter, net_channel, net_send, net_ready, notify, time_of_day, set_time_of_day, time_speed_control, set_time_speed, game_settings, set_game_settings, damage_filter, buff_types, buffs, apply_buff, remove_buff, knowledge, grant_knowledge, quests, weather_at, weather_fronts, set_weather, terrain_materials, terrain_edit, config_schema, summoned, item_data_open, item_data_get, item_data_set, ui_frame, ui_widget, ui_dialog, ui_hud, ui_markers, config_check, texts, content_serve, content_read, icons_ready, stat_open, stat_info, stat_value, stat_modify, stat_buff, stats_changed, stat_sources, stat_watch, define_event, subscribe_event, send_event, affix_open, affix_roll, item_lines, affix_info, migrate, declare_content, entity_data_open, replaced, knowledge_input_filter, language, keep_game_settings, forget_game_settings, revoke_knowledge, account_knowledge, resource_read, game_text, rebuild_resource, recipe_rule, recipe_disabled, define_point, attach_point, run_point, actions, event_types, recipe_kind, feature, rate, set_rate, change_data, loop_wait, server_wait, task_workers, altar_zone_filter, altar_zones_changed, altar_limit_filter, base_limit, voxel_read, declare_change, set_change, build_rights, transaction_commit, transaction_undo, transaction_status, transaction_commit_for, world_quiet, fog_open, fog_read, serve_call, find_call, call, transaction_entities, world_persists, map_markers, account_action, allowlist_set, allowlist_has, subscribe_tick, set_tick_every, map_frame, request_base_limits, map_marker_types, use_content, chat_mute_set, chat_mute_has },
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
            let _ = write!(text, "{name} size {} align {}", shape.size, shape.align);
            if let Some(born) = self.born.get(name) {
                let _ = write!(text, " born {born}");
            }
            text.push('\n');
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
            born: BTreeMap::new(),
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
                let (name, size, align) = match words[..] {
                    [name, "size", size, "align", align] => (name, size, align),
                    [name, "size", size, "align", align, "born", born] => {
                        layout.born.insert(name.to_owned(), born.parse().ok()?);
                        (name, size, align)
                    }
                    _ => return None,
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

/// An ABI major and minor.
type Version = (u16, u16);

/// The golden file of `layout` at `version`: the version line, then the
/// types.
fn render_golden((major, minor): Version, layout: &Snapshot) -> String {
    format!("abi {major}.{minor}\n{}", layout.render())
}

/// The version and the layout of a golden file.
fn parse_golden(text: &str) -> Option<(Version, Snapshot)> {
    let (first, rest) = text.split_once('\n')?;
    let (major, minor) = first.strip_prefix("abi ")?.split_once('.')?;
    let version = (major.parse().ok()?, minor.parse().ok()?);
    Some((version, Snapshot::parse(rest)?))
}

/// Whether `new` only appends to `old`: every old type keeps its alignment
/// and its fields, a type grows only at its end and only when it starts with
/// a [`Header`], which tells the reader its writer's size; new types are
/// free. An appended field starts past the old size, since an older writer's
/// header covers its padding.
fn appends(old: &Snapshot, new: &Snapshot) -> bool {
    old.types.iter().all(|(name, was)| {
        new.types.get(name).is_some_and(|now| {
            let headed = was.fields.first().is_some_and(|field| field.ty == "Header");
            let past_old_end = now.fields[now.fields.len().min(was.fields.len())..]
                .iter()
                .all(|field| field.offset >= was.size);
            now.align == was.align
                && now.fields.starts_with(&was.fields)
                && past_old_end
                && (now == was || headed && now.size > was.size)
        })
    })
}

/// Why `now` is the wrong version for the layout `new`, when the golden
/// file holds `old` at `was`; `None` when it fits. The next major at minor 0
/// fits any change; a lower or a skipped one could match mods of an earlier
/// major. Within a major, an unchanged layout keeps the minor, an append
/// takes the next one, and any other change fits no minor.
fn wrong_bump(was: Version, old: &Snapshot, new: &Snapshot, now: Version) -> Option<String> {
    let (major, minor) = was;
    let rewrite = "then run the tests with ENSHWICK_ABI_LAYOUT=update";
    if now.0 != major {
        if now.0 != major + 1 {
            return Some(format!(
                "the ABI major after {major} is {}: set ABI_MAJOR to it, {rewrite}",
                major + 1
            ));
        }
        if now.1 != 0 {
            return Some(format!(
                "a new ABI major starts at minor 0: set ABI_MINOR to 0, {rewrite}"
            ));
        }
        return wrong_born(old, new, true);
    }
    let needed = if old.types == new.types {
        minor
    } else if appends(old, new) {
        minor + 1
    } else {
        return Some(format!(
            "the layout changed past an append to ABI {major}.{minor}: \
             set ABI_MAJOR to {} and ABI_MINOR to 0, {rewrite}",
            major + 1
        ));
    };
    if now.1 != needed {
        return Some(format!(
            "the layout from ABI {major}.{minor} needs ABI {major}.{needed}: \
             set ABI_MINOR to {needed}, {rewrite}"
        ));
    }
    wrong_born(old, new, false)
}

/// Why a headed type's [`Headed::BORN`](crate::abi::Headed::BORN) in `new`
/// is wrong, or `None`: in a new major and for a new type it is the type's
/// size; within a major it keeps what the golden file `old` holds, or the old
/// size of a type that becomes headed, so a writer of an older minor still
/// covers what readers require, and a type stays headed for the rest of its
/// major.
fn wrong_born(old: &Snapshot, new: &Snapshot, new_major: bool) -> Option<String> {
    if !new_major {
        let lost = old.born.keys().find(|name| !new.born.contains_key(*name));
        if let Some(name) = lost {
            return Some(format!(
                "{name} stopped being headed within an ABI major: older writers still send \
                 it with a header, so keep impl_headed!({name}) or set a new ABI_MAJOR"
            ));
        }
    }
    new.born.iter().find_map(|(name, &born)| {
        let size = new.types.get(name).map_or(born, |shape| shape.size);
        let kept = (!new_major)
            .then(|| {
                old.born
                    .get(name)
                    .copied()
                    .or_else(|| old.types.get(name).map(|shape| shape.size))
            })
            .flatten();
        let want = kept.unwrap_or(size);
        (born != want).then(|| {
            format!(
                "{name} has Headed::BORN {born} and needs {want}, the end of what every \
                 writer of this ABI major knows: write impl_headed!({name}, born = {want})"
            )
        })
    })
}

/// The golden file's text, `None` only when it does not exist; any other
/// error, such as a lock or a denied read, is an error, so the update mode
/// never writes over a file it could not read.
fn read_golden(path: &Path) -> std::io::Result<Option<String>> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn golden_path() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src/abi/layout.txt")
}

#[test]
fn the_abi_layout_and_version_match_the_golden_file() {
    let new = current();
    let path = golden_path();
    let now = (ABI_MAJOR, ABI_MINOR);
    let rendered = render_golden(now, &new);
    let update = std::env::var_os("ENSHWICK_ABI_LAYOUT").is_some_and(|value| value == "update");
    let read = read_golden(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let Some(text) = read else {
        // A missing file holds no version to bump from.
        assert!(
            update,
            "{} is missing: run the tests with ENSHWICK_ABI_LAYOUT=update",
            path.display()
        );
        std::fs::write(&path, rendered).unwrap();
        return;
    };
    // A broken file, such as one left by a merge conflict, is mended by hand
    // so the bump check sees both sides.
    let (was, old) = parse_golden(&text).unwrap_or_else(|| {
        panic!(
            "{} does not read as a golden file: resolve it by hand, keeping its version line",
            path.display()
        )
    });
    if let Some(why) = wrong_bump(was, &old, &new, now) {
        panic!("{why}");
    }
    if update {
        std::fs::write(&path, rendered).unwrap();
        return;
    }
    assert!(
        text == rendered,
        "the ABI layout or version differs from {}: run the tests with ENSHWICK_ABI_LAYOUT=update and commit the file",
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
fn only_a_missing_golden_file_counts_as_missing() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    assert!(matches!(
        read_golden(&src.join("abi/no-such-layout.txt")),
        Ok(None)
    ));
    assert!(matches!(read_golden(&golden_path()), Ok(Some(_))));
    assert!(
        read_golden(&src).is_err(),
        "a folder reads as an error, not as missing"
    );
}

#[test]
fn a_rendered_golden_file_reads_back() {
    let layout = current();
    assert_eq!(
        parse_golden(&render_golden((5, 2), &layout)),
        Some(((5, 2), layout))
    );
}

/// Pairs of structs, each a change a layout can see.
mod fixture {
    use super::Header;

    #[repr(C)]
    pub(super) struct Before {
        pub(super) call: extern "system" fn(u32) -> u32,
    }

    #[repr(C)]
    pub(super) struct After {
        pub(super) call: extern "system" fn(u64, u32) -> u32,
    }

    #[repr(C)]
    pub(super) struct Headed {
        pub(super) header: Header,
        pub(super) value: u32,
    }

    #[repr(C)]
    pub(super) struct HeadedGrown {
        pub(super) header: Header,
        pub(super) value: u32,
        pub(super) added: u32,
    }

    #[repr(C)]
    pub(super) struct Padded {
        pub(super) header: Header,
        pub(super) wide: u64,
        pub(super) short: u32,
    }

    #[repr(C)]
    pub(super) struct PaddedGrown {
        pub(super) header: Header,
        pub(super) wide: u64,
        pub(super) short: u32,
        pub(super) added: u32,
        pub(super) more: u64,
    }

    #[repr(C)]
    pub(super) struct Plain {
        pub(super) value: u32,
    }

    #[repr(C)]
    pub(super) struct PlainGrown {
        pub(super) value: u32,
        pub(super) added: u32,
    }
}

use fixture::{After, Before, Headed, HeadedGrown, Padded, PaddedGrown, Plain, PlainGrown};

/// A layout of the named shapes.
fn layout(types: &[(&str, Shape)]) -> Snapshot {
    Snapshot {
        types: types
            .iter()
            .map(|(name, shape)| ((*name).to_owned(), shape.clone()))
            .collect(),
        born: BTreeMap::new(),
    }
}

/// `layout` with `Fixture`'s [`Headed::BORN`](crate::abi::Headed::BORN) at `born`.
fn born_at(mut layout: Snapshot, born: usize) -> Snapshot {
    layout.born.insert("Fixture".to_owned(), born);
    layout
}

/// Whether `now` fits the change of one type `Fixture` from `old` at ABI
/// 5.2 to `new`.
fn fits(old: Shape, new: Shape, now: Version) -> bool {
    let old = layout(&[("Fixture", old)]);
    let new = layout(&[("Fixture", new)]);
    wrong_bump((5, 2), &old, &new, now).is_none()
}

#[test]
fn an_unchanged_layout_keeps_its_version() {
    let headed = || shape!(Headed { header, value });
    assert!(fits(headed(), headed(), (5, 2)));
    assert!(!fits(headed(), headed(), (5, 3)));
    assert!(fits(headed(), headed(), (6, 0)));
}

#[test]
fn a_field_appended_to_a_headed_struct_needs_the_next_minor() {
    let old = || shape!(Headed { header, value });
    let new = || {
        shape!(HeadedGrown {
            header,
            value,
            added
        })
    };
    assert!(fits(old(), new(), (5, 3)));
    assert!(!fits(old(), new(), (5, 2)), "a missing bump fails");
    assert!(!fits(old(), new(), (5, 4)), "a skipped minor fails");
    assert!(fits(old(), new(), (6, 0)));
}

#[test]
fn a_field_appended_into_the_old_tail_padding_needs_a_new_major() {
    let old = || {
        shape!(Padded {
            header,
            wide,
            short
        })
    };
    let new = || {
        shape!(PaddedGrown {
            header,
            wide,
            short,
            added,
            more
        })
    };
    assert!(new().fields[3].offset < old().size);
    assert!(!fits(old(), new(), (5, 3)));
    assert!(fits(old(), new(), (6, 0)));
}

#[test]
fn the_major_moves_only_to_the_next_one() {
    let headed = || shape!(Headed { header, value });
    assert!(!fits(headed(), headed(), (4, 0)));
    assert!(!fits(headed(), headed(), (1, 0)));
    assert!(!fits(headed(), headed(), (7, 0)), "a skipped major fails");
}

#[test]
fn an_append_keeps_what_readers_require_of_older_writers() {
    let old = born_at(layout(&[("Fixture", shape!(Headed { header, value }))]), 12);
    let grown = layout(&[(
        "Fixture",
        shape!(HeadedGrown {
            header,
            value,
            added
        }),
    )]);
    assert_eq!(
        wrong_bump((5, 2), &old, &born_at(grown.clone(), 12), (5, 3)),
        None
    );
    let moved = wrong_bump((5, 2), &old, &born_at(grown.clone(), 16), (5, 3)).unwrap();
    assert!(
        moved.contains("impl_headed!(Fixture, born = 12)"),
        "{moved}"
    );
    assert_eq!(
        wrong_bump((5, 2), &old, &born_at(grown.clone(), 16), (6, 0)),
        None
    );
    assert!(
        wrong_bump((5, 2), &old, &born_at(grown, 12), (6, 0)).is_some(),
        "a new major starts again at the size"
    );
}

#[test]
fn a_type_that_becomes_headed_in_an_append_is_born_at_its_old_size() {
    let old = layout(&[("Fixture", shape!(Headed { header, value }))]);
    let grown = layout(&[(
        "Fixture",
        shape!(HeadedGrown {
            header,
            value,
            added
        }),
    )]);
    assert!(wrong_bump((5, 2), &old, &born_at(grown.clone(), 16), (5, 3)).is_some());
    assert_eq!(wrong_bump((5, 2), &old, &born_at(grown, 12), (5, 3)), None);
}

#[test]
fn a_type_stays_headed_for_the_rest_of_its_major() {
    let headed = || layout(&[("Fixture", shape!(Headed { header, value }))]);
    let old = born_at(headed(), 12);
    assert!(wrong_bump((5, 2), &old, &headed(), (5, 2)).is_some());
    assert_eq!(wrong_bump((5, 2), &old, &headed(), (6, 0)), None);
}

#[test]
fn the_born_sizes_are_those_of_the_headed_types() {
    let layout = current();
    assert!(
        layout.born.len() > 50,
        "{} headed types found",
        layout.born.len()
    );
    for name in layout.born.keys() {
        let first = layout.types[name].fields.first();
        assert!(first.is_some_and(|field| field.ty == "Header"), "{name}");
    }
}

#[test]
fn a_new_type_needs_the_next_minor() {
    let old = layout(&[("Headed", shape!(Headed { header, value }))]);
    let new = layout(&[
        ("Headed", shape!(Headed { header, value })),
        ("Plain", shape!(Plain { value })),
    ]);
    assert_eq!(wrong_bump((5, 2), &old, &new, (5, 3)), None);
    assert!(wrong_bump((5, 2), &old, &new, (5, 2)).is_some());
}

#[test]
fn a_removed_field_needs_a_new_major() {
    let old = || {
        shape!(HeadedGrown {
            header,
            value,
            added
        })
    };
    let new = || shape!(Headed { header, value });
    assert!(!fits(old(), new(), (5, 3)));
    assert!(fits(old(), new(), (6, 0)));
    assert!(!fits(old(), new(), (6, 1)), "a new major starts at minor 0");
}

#[test]
fn a_removed_type_needs_a_new_major() {
    let old = layout(&[
        ("Headed", shape!(Headed { header, value })),
        ("Plain", shape!(Plain { value })),
    ]);
    let new = layout(&[("Headed", shape!(Headed { header, value }))]);
    assert!(wrong_bump((5, 2), &old, &new, (5, 3)).is_some());
    assert_eq!(wrong_bump((5, 2), &old, &new, (6, 0)), None);
}

#[test]
fn a_struct_without_a_header_grows_only_in_a_new_major() {
    let old = || shape!(Plain { value });
    let new = || shape!(PlainGrown { value, added });
    assert!(!fits(old(), new(), (5, 3)));
    assert!(fits(old(), new(), (6, 0)));
}

#[test]
fn a_changed_function_signature_needs_a_new_major() {
    assert_eq!(size_of::<Before>(), size_of::<After>());
    let old = || shape!(Before { call });
    let new = || shape!(After { call });
    assert!(!fits(old(), new(), (5, 3)));
    assert!(fits(old(), new(), (6, 0)));
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
fn the_c_header_defines_this_abi_version() {
    let header =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("include/enshwick.h"))
            .unwrap();
    for define in [
        format!("#define ENSHWICK_ABI_MAJOR {ABI_MAJOR}"),
        format!("#define ENSHWICK_ABI_MINOR {ABI_MINOR}"),
    ] {
        assert!(
            header.lines().any(|line| line == define),
            "include/enshwick.h lacks `{define}`: run `cargo xtask header`"
        );
    }
}
