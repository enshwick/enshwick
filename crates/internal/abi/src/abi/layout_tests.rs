//! The layout of every ABI type, held against the committed golden file
//! `layout.txt`, whose first line is the [`ABI_MAJOR`] and [`ABI_MINOR`] it
//! was written at. A change of the layout needs the next minor when it only
//! appends, else a new major, once the ABI is released ([`ABI_RELEASED`]);
//! before that, any change keeps the version.
//!
//! After an intended change, set the version the failing test names and run
//! the tests with `ENSHWICK_ABI_LAYOUT=update` to rewrite the file.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use super::*;
use crate::content::Guid;

mod source;

use source::{constants_in_source, structs_in_source};

/// Whether the ABI at [`ABI_MAJOR`] and [`ABI_MINOR`] has shipped in a
/// release. Until it has, every layout change, a breaking one too, keeps
/// the version and rewrites the golden file at it; from the release on,
/// each change takes the version [`wrong_bump`] asks for. The ABI ships as
/// 1.0, and the release sets this to true (the maintainer, 2026-11-26).
const ABI_RELEASED: bool = false;

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
/// [`Headed::BORN`](crate::abi::Headed::BORN) of each headed type and the
/// value of every constant, since a renumbered status or kind breaks a mod as
/// surely as a moved field.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Snapshot {
    types: BTreeMap<String, Shape>,
    born: BTreeMap<String, usize>,
    constants: BTreeMap<String, String>,
}

/// A type the macro `shapes!` asks for its [`Headed::BORN`](crate::abi::Headed::BORN): through
/// [`BornOfHeaded`] for a headed type, else through [`BornOfOther`], chosen
/// by autoref on `&&Probe`.
struct Probe<T>(core::marker::PhantomData<T>);

trait BornOfHeaded {
    fn born(&self) -> Option<usize>;
}

impl<T: Headed> BornOfHeaded for &Probe<T> {
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
            Snapshot { types, born, constants: constants_in_source() }
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
        PlayerDeathData { header, entity, cause, key, position, cause_guid, killer_entity, reserved_tail },
        WaypointData { header, key, entity, position, set, reserved_tail },
        SoundPlay { header, container, place, entity, at, volume, fade_in_ms },
        SoundSteer { header, kind, volume, fade_ms, reserved, at },
        SoundInfo { header, state, reason },
        SoundAudience { header, kind, radius, player, at },
        KnowledgeData { header, entity, id, value },
        QuestData { header, entity, quest },
        QuestStepData { header, entity, quest, step },
        QuestProgressData { header, entity, quest, step, part, value, target },
        QuestPayment { header, entity, quest, count, limit, allowed, reserved, experience },
        KnowledgeInput { header, character, id, kind, scope, reserved, value, refusal, allowed, reserved_tail },
        ChunkData { header, x, z, size },
        TerrainPageData { header, x, z, size },
        BuildingData { header, entity, character, item, kind, replay, template, position },
        PlayerInfo { header, slot, entity, key, permissions, name_len, name },
        TickData { header, step, elapsed_steps, elapsed_ns, step_ns, cost_ns, game },
        TickHandle { 0 },
        PlayerData { header, slot, entity, key },
        ActionCall { header, kind, language, player, action, args, target, reply, word, via, reason },
        ActionReply { header, done, text },
        ActionTarget { header, flags, entity, position },
        CommandInfo { header, name, alias, mod_id },
        RenameCall { header, entity, player, text },
        Guid { 0 },
        ResourceKey { guid, type_hash, part },
        Fingerprint { guid, age },
        BuildInfo { header, side, base, fingerprint, complete },
        Symbol { name, rva },
        BuildRow { side, fingerprint, symbols, symbol_count, constants, constant_count },
        Relation { 0 },
        Dependency { id, range, relation },
        ManifestFlags { 0 },
        Manifest { header, id, version, side, flags, dependencies, dependency_count, builds, build_count, name, description, authors, author_count, license, homepage, points, exports, export_count },
        HookFlags { 0 },
        HookSpec { header, target, detour, next, priority, flags },
        CallerFrame { rsp, rbx, rbp, rdi, rsi, r12, r13, r14, r15 },
        ModInfo { header, id, version, side, flags, off, file, name, description, authors, author_count, license, homepage },
        ModHandle { 0 },
        Registers { rax, rcx, rdx, rbx, rsp, rbp, rsi, rdi, r8, r9, r10, r11, r12, r13, r14, r15, rflags },
        MidHookSpec { header, address, handler, user, priority, flags },
        InventoryOp { kind, flags, target, slot, other, fill, item, count },
        InventoryRequest { header, flags, actor, ops, op_count, reserved },
        InventoryOpResult { error, remaining },
        InventoryResult { header, outcome, op_count, ops },
        ItemTypeInfo { header, id, max_stack, category, rarity, flags, placed_entity },
        Bounds { min, max },
        Extent { min, max },
        AltarInfo { header, entity, level, tilted, level_count, reserved, zone, levels, position },
        AltarLimit { header, flame_level, vanilla, allowed, reserved },
        BaseLimitInfo { header, zones, bases, vanilla_zones, reserved, vanilla_bases, max_zones, max_bases },
        AltarZone { header, altar, level, reserved, grow, height },
        BuildingZoneCheck { header, zones, zone_count, object, margin },
        SurvivorLookup { header, survivor, vanilla, purpose, entity, cell, bed_of, token },
        BedCheck { header, npc_id, entity, bed_id, home, bed_at },
        Summon { header, item, ammo, template, target, slot, pide, meter, consumed, reserved },
        Interaction { header, kind, character, target, item, known, reserved, position, moved, stack, amount, from, to, action, transfer, moving, reserved_move, from_slot, to_slot },
        InteractionPrompt { header, item, verb, text, flags },
        ContainerPut { header, container, container_item, item, count, slot },
        SkillCheck { header, node, gate, unlocked, reserved },
        Comfort { header, category, value, position },
        ExperienceRow { entity, level, progress, need, gained },
        ExperiencePass { header, rows, row_count, level_cap, need_for, token },
        GameEventData { header, type_hash, size, data },
        EntityPlace { entity, reserved, position },
        TeleportRequest { header, entity, flags, position, orientation },
        RespawnPointInfo { header, entity, kind, position, orientation },
        SpawnRequest { header, template, position, orientation, level, flags, lifetime_ns },
        DropRequest { header, item, count, position, level, rarity },
        ProgressWriteData { header, entity, what, value, outcome, level, experience, gained },
        CraftCheck { header, crafting_operator_id, crafting_station_id, recipe_id, recipe_amount, reserved },
        BuildCheck { header, character, item, position, reason, account },
        NetMessage { header, channel, reserved, peer, data, size, reserved_tail },
        DaytimeInfo { header, time_ns, day_start_ns, day_end_ns, day_length_ns, night_length_ns, speed, flags },
        DamageCheck { header, cause, target, source, attacker, total, amount_count, amounts },
        StatSpec { header, ty, format, reserved, default, min, max, group, flags, reserved_display },
        StatRef { index, key },
        StatModifier { stat, op, reserved, value },
        StatModify { header, entity, key, modifier, flags, duration_ms },
        StatChange { header, entity, stat, old, new, reserved },
        StatSource { kind, op, reserved, stat, value, id, owner },
        AffixSpec { header, group, modifiers, modifier_count, weight, categories, rarity, reserved },
        AffixModifierSpec { stat, op, reserved, min, max, step },
        AffixRoll { header, pide, item, min, max, max_rarity, flags, seed },
        ItemLine { modifier, affix },
        BuffTypeInfo { header, id, slot, flags, name, duration_ns },
        ActiveBuff { header, id, entity, slot, level, duration_ns, remaining_ns },
        BuffRequest { header, entity, id, level, reserved, duration_ns },
        BuffTypeAdd { header, guid, name, description, icon, life_time_ns, like, changes, slot, flags },
        KnowledgeInfo { header, id, kind, scope, reserved, value, flags },
        KnowledgeChange { header, account, entity, id, value, kind, op, reserved },
        WeatherFront { header, layer, effect, position, radius, reserved, remaining_ns, phenomenon },
        WeatherSample { header, clear, rain, snow, blizzard },
        TerrainMaterialInfo { header, id, flags, hardness, reserved, item, health },
        TerrainEdit { header, kind, shape, flags, material, position, radius, height, protect },
        TerrainMaterialAdd { header, guid, item, health, like, fallback, changes, hardness, flags, reserved },
        Voxel { kind, material, fill, reserved },
        VoxelChange { at, before, after },
        VoxelEditRequest { header, by, min, size, changes, count },
        BuildRights { header, character, refusal, min, max },
        TransactionOp { kind, reserved, data },
        TransactionInfo { header, state, reason, ops, applied, bricks, refusal, reserved, holds, created },
        PropPlace { header, item, reserved, position, orientation },
        PropPlacementInfo { header, template, min, max, flags },
        PropRemove { header, entity, flags },
        VoxelWrite { header, min, size, voxels, count },
        VoxelRegion { header, min, size },
        BlockPlace { header, shape, material, position, rotation },
        LocalInputInfo { header, flags, reserved, age_ms },
        GameBinding { header, first, second },
        LoaderOff { header, name, reason, at },
        ClientCommand { header, kind, hold_ms, session, expected_screen, target, look, direction, recipe, reserved, cancels, password, position, world, character, online, item, text, answer, dialog, slot, target_slot, entity, amount, key, key_slot },
        ClientCommandStatus { header, state, reason, kind, reserved, age_ms, phase, failure },
        ClientSnapshot { header, state, available, session, age_ms, scene, hud_screen, return_screen, menu_tab, held, player, host, highlighted_recipe, reserved, server, loader_screen_mod, loader_screen, reserved2, dialog_mod, dialog_buttons, dialog, prompt_target, prompt_verb, character, reserved3, position, look, health, health_max, stamina, stamina_max, actionbar_slot, reserved4 },
        ClientWindowItem { header, kind, field, flags, reserved, window, item, label, line },
        ClientSlot { header, slot, item, count, pide, category, flags, reserved },
        UiFrame { header, token, screen, reserved, hud_screen, prompt_title, prompt_verb, screen_host, screen_host_template, screen_host_position, hovered_slot, hovered_item, hovered_count, hovered_pide, reserved_hovered },
        UiTooltipItem { header, token, item, pide },
        UiWidget { header, kind, flags, style, steps, text, detail, options, option_count, index, value, min, max, result, anchor, reserved, sound, item, count },
        UiMarker { header, position, label, color, marker_type },
        UiDialog { header, title, message, detail, buttons, flags },
        FogInfo { header, width, height, metres_per_pixel, size_z, version, session, uncovered, uncovered_pixels, reserved, uncovered_min, uncovered_size },
        MapMarkerInfo { header, position, marker_type, label },
        MapFrameInfo { header, origin, size, metres_per_cell },
        MapMarkerTypeInfo { header, type_name, marker_id, sorting_category, is_fast_travel_destination, is_custom_marker_type, show_above_fog_of_war, show_label, reserved },
        MarkerTypeSorting { 0 },
        TypeAttribute { name, value_type, value },
        TypeConstant { name, value },
        TypeField { name, type_name, offset, attribute_count, attributes },
        TypeEntry { header, name, impact_name, qualified_name, inner, internal_hash, size, align, element_align, kind, flags, reserved, count, field_count, constant_count, attribute_count, fields, constants, attributes },
        HostApi { header, host_version, build, symbol, system, field_offset, type_size, hook, caller_frame, fault, log, settings, publish, service, constant, data_dir, log_level, subscribe, type_hash, component, entity_template, mods, code_range, patch_code, workshop_recipes, hosting, server_world, join_missing, hook_at, status, subscribe_game_event, net_channel, net_send, net_ready, config_schema, config_check, texts, define_event, subscribe_mod_event, send_event, rename_content, mod_dir, language, define_point, attach_point, run_point, event_types, feature, loader_off, loop_wait, server_wait, task_workers, serve_call, find_call, call, subscribe_tick, set_tick_every, set_config, subscribe_client_game_event, serve_resource, add_text, define_item_data, item_data_get, set_item_data, content_serve, content_read, icons_ready, declare_content, define_entity_data, resource_read, game_text, rebuild_resource, rekey_resource, recipe_rule, recipe_disabled, recipe_kind, rate, set_rate, change_data, set_change, use_content, resource_keys, content_hashes, types, game_version, crafting_prop, read_only, resource_served, resource_buffer, content_name, player, players, chat, chat_all, set_label, rename_filter, item_info, request_altars, altars, building_zone_filter, base_zones, entities, destroy, save_world, world_key, component_write, notify, time_of_day, set_time_of_day, request_time_speed, set_time_speed, game_settings, set_game_settings, buff_types, buff_type_add, buffs, apply_buff, remove_buff, weather_at, weather_fronts, set_weather, terrain_materials, terrain_material_add, terrain_edit, altar_zone_filter, altar_zones_changed, altar_limit_filter, base_limit, voxel_read, build_rights, change_permissions, transaction_commit, transaction_undo, transaction_status, world_quiet, transaction_entities, transaction_settled, prop_placement_info, world_persists, map_markers, account_action, map_frame, waypoint, request_base_limits, set_chat_mute, chat_muted, set_base_limit, save_info, hold_saves, set_saved, progress_write, server_stop, fog_uncover, language_of, join_rule, request_inventory, inventory_submit, container_rule, show_pickup, survivor_lookup, survivor_bed_check, summon_filter, summon_item, survivor_status, emit, teleport, spawn, respawn_point, set_respawn_point, damage_filter, damage_causes_off, summoned, define_stat, stat_info, stat_value, stat_modify, stat_buff, stats_changed, stat_sources, stat_watch, define_affix, affix_roll, item_lines, affix_info, item_filter, drop_items, skill_override, comfort_filter, experience_filter, knowledge, change_knowledge, knowledge_set, quests, quest_reward_filter, knowledge_input_filter, game_query, run_console, interaction_filter, interaction_prompt, craft_filter, build_filter, actions, control, run_action, chat_filter, commands, run_command, command_list, actions_changed, action_window, local_held, local_input_status, client_snapshot, client_command, client_command_status, client_windows, client_slots, game_binding, game_rebind, ui_frame, ui_widget, ui_dialog, ui_hud, ui_markers, request_fog, fog_read, map_marker_types, ui_extension, ui_item_tooltip, sound_play, sound_steer, sound_state, sound_play_for, sound_steer_for },
        SaveInfo { header, world, saved_at, size, backups, reserved },
        ChatLine { header, account, entity, kind, loca, text },
        ChatRewrite { header, buffer, capacity, len },
        UiExtension { header, screen, edge, reserved, hosts, host_count, reserved_hosts, align, shift, inset, reserved_place },
        ItemAction { header, kind, character, item, count, inventory, slot },
        ControlSpec { header, name, label, key, second, link, fires, hold_ms, action },
        ControlEvent { header, name, kind, server, reserved, player },
        AccountActionType { 0 },
        JoinCall { header, account },
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
        ContentRename { from, to },
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
        for (name, value) in &self.constants {
            let _ = writeln!(text, "const {name} = {value}");
        }
        text
    }

    fn parse(text: &str) -> Option<Self> {
        let mut layout = Self {
            types: BTreeMap::new(),
            born: BTreeMap::new(),
            constants: BTreeMap::new(),
        };
        let mut last = None;
        for line in text.lines() {
            let words: Vec<&str> = line.split_whitespace().collect();
            if let Some(constant) = line.strip_prefix("const ") {
                let (name, value) = constant.split_once(" = ")?;
                layout.constants.insert(name.to_owned(), value.to_owned());
            } else if let Some(field) = line.strip_prefix("    ") {
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
/// header covers its padding. Every old constant keeps its value; new ones are
/// free.
fn appends(old: &Snapshot, new: &Snapshot) -> bool {
    let kept = old
        .constants
        .iter()
        .all(|(name, value)| new.constants.get(name) == Some(value));
    kept && old.types.iter().all(|(name, was)| {
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
/// major, except the one step from the last of the [`ALPHA_MAJORS`] to the
/// stable 1.0. Within a major, an unchanged layout keeps the minor,
/// an append takes the next one, and any other change fits no minor.
fn wrong_bump(was: Version, old: &Snapshot, new: &Snapshot, now: Version) -> Option<String> {
    let (major, minor) = was;
    let rewrite = "then run the tests with ENSHWICK_ABI_LAYOUT=update";
    let stable = major == *ALPHA_MAJORS.end() && now.0 == 1;
    let next = next_major(major);
    if now.0 != major {
        if now.0 != next && !stable {
            return Some(format!(
                "the ABI major after {major} is {next}: set ABI_MAJOR to it, {rewrite}"
            ));
        }
        if now.1 != 0 {
            return Some(format!(
                "a new ABI major starts at minor 0: set ABI_MINOR to 0, {rewrite}"
            ));
        }
        return wrong_born(old, new, true);
    }
    let needed = if old.types == new.types && old.constants == new.constants {
        minor
    } else if appends(old, new) {
        minor + 1
    } else {
        return Some(format!(
            "the layout changed past an append to ABI {major}.{minor}: \
             set ABI_MAJOR to {next} and ABI_MINOR to 0, {rewrite}"
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

/// Why `now` is the wrong version for the layout `new`, the golden file
/// holding `old` at `was`, with the ABI `released` or not: before its
/// release every change keeps the version, its types born at their size
/// as in a new major, since no released mod relies on the older layout;
/// otherwise as [`wrong_bump`] says.
fn wrong_change(
    was: Version,
    old: &Snapshot,
    new: &Snapshot,
    now: Version,
    released: bool,
) -> Option<String> {
    if released {
        return wrong_bump(was, old, new, now);
    }
    if now != was {
        let (major, minor) = was;
        return Some(format!(
            "until the ABI's release a layout change keeps ABI {major}.{minor}: \
             set ABI_MAJOR to {major} and ABI_MINOR to {minor}, \
             then run the tests with ENSHWICK_ABI_LAYOUT=update"
        ));
    }
    wrong_born(old, new, true)
}

/// The major after `major`: the next number, past the [`ALPHA_MAJORS`] a
/// stable major never takes.
fn next_major(major: u16) -> u16 {
    if major + 1 == *ALPHA_MAJORS.start() {
        ALPHA_MAJORS.end() + 1
    } else {
        major + 1
    }
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
    if let Some(why) = wrong_change(was, &old, &new, now, ABI_RELEASED) {
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
    // The header ships with the API crate, beside this one.
    let header = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../enshwick/include/enshwick.h");
    let header = std::fs::read_to_string(header).unwrap();
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

mod bump;
