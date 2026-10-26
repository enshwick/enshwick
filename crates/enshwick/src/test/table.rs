//! The loader's table, answering from the installed fake.

use std::sync::{Mutex, MutexGuard};

use crate::abi::{
    ActiveBuff, AltarInfo, AltarLimitFn, AltarZoneSize, BedCheckFn, BuffTypeInfo, BuildCheckFn,
    ComfortFn, ConfigCheckFn, CraftCheckFn, DamageFilterFn, DaytimeInfo, Event, EventFn,
    ExperiencePassFn, GameEventFn, Header, HostApi, InteractionFn, InventoryResultFn,
    KnowledgeInfo, KnowledgeInputFn, Level, MidHookFn, ModEventFn, NetMessageFn, RenameCallFn,
    ResourceKey, ResourcePatchFn, SkillCheckFn, SpawnRequest, Status, StatusFn, Str, SummonFn,
    SurvivorLookupFn, SurvivorStatusFn, TeleportRequest, TerrainEdit, TerrainMaterialInfo,
    UiAnswerFn, UiFrameFn, WeatherFront, ZoneCheckFn,
};
use crate::locks::Relock;

use super::{FakeHost, LinkedHook, ServedResource, SubmittedRequest};

mod build;
mod content;
mod gameplay;
mod hooks;
mod inventory;
mod mods;
mod net;
mod services;
mod ui;
mod world;

use build::{
    build, caller_frame, constant, data_dir, field_offset, log_level, symbol, system, type_hash,
    type_size,
};
use content::{
    add_text, content_read, content_serve, entity_data_open, game_text, icons_ready, item_data_get,
    item_data_open, item_data_set, rebuild_resource, recipe_disabled, recipe_rule, resource_read,
    serve_resource,
};
use gameplay::{
    account_knowledge, altar_limit, altar_zones_control, altars, altars_open, apply_buff,
    base_zones, buff_types, buffs, build_filter, comfort_filter, craft_filter, damage_filter,
    experience_filter, grant_knowledge, interaction_filter, knowledge, knowledge_input_filter,
    quests, raise_base_limit, remove_buff, revoke_knowledge, set_altar_zones, skill_override,
    summon_filter, summon_item, summoned, survivor_bed_check, survivor_lookup, survivor_status,
    zone_check_filter,
};
use hooks::{code_range, hook, hook_at, patch_code, workshop_recipes};
use inventory::{inventory_open, inventory_submit, item_info};
use mods::{
    config_check, config_schema, fault, join_missing, language, log, migrate, mods, replaced,
    retired_command, settings, sync, texts,
};
use net::{net_channel, net_ready, net_send};
pub(crate) use services::view;
use services::{
    declare_content, define_event, publish, send_event, service, subscribe, subscribe_event,
};
use ui::{ui_dialog, ui_frame, ui_hud, ui_markers, ui_widget};
use world::{
    actions, chat, component, component_write, destroy, emit, entities, entity_template,
    event_types, feature, forget_game_settings, game_event, game_settings, hosting,
    keep_game_settings, notify, player, players, rename_filter, run_console, save_world,
    server_world, set_game_settings, set_label, set_time_of_day, set_time_speed, set_weather,
    spawn, status, teleport, terrain_edit, terrain_materials, time_of_day, time_speed_control,
    weather_at, weather_fronts, world_key,
};

/// A submitted inventory request and where its result goes.
pub(crate) struct OpenRequest {
    pub(crate) request: SubmittedRequest,
    pub(crate) done: InventoryResultFn,
    pub(crate) user: usize,
}

/// A mod event handler: name, layout, handler and the mod's pointer.
pub(crate) type ModEventSubscription = (String, Vec<(String, u64)>, ModEventFn, usize);

/// What the installed fake answers and what it has recorded.
pub(crate) struct State {
    pub(crate) host: FakeHost,
    pub(crate) logs: Vec<(Level, String)>,
    pub(crate) faults: Vec<String>,
    pub(crate) hooks: Vec<LinkedHook>,
    /// Mid-function hooks: address, handler and the mod's pointer.
    pub(crate) mid_hooks: Vec<(u64, MidHookFn, usize)>,
    pub(crate) subscriptions: Vec<(Event, EventFn, usize)>,
    /// Game event handlers: type hash (0 for every type), handler, pointer.
    pub(crate) game_events: Vec<(u32, GameEventFn, usize)>,
    pub(crate) served: Vec<(ServedResource, ResourcePatchFn, usize)>,
    /// Every rebuild of a served resource the mod asked for, at init and
    /// after.
    pub(crate) rebuilds: Vec<ResourceKey>,
    /// The mod's recipe rules in force: kind and name.
    pub(crate) recipe_rules: Vec<(crate::abi::RecipeRuleKind, String)>,
    pub(crate) texts: Vec<(u32, u32, String)>,
    pub(crate) code_patches: Vec<(u64, Vec<u8>, Vec<u8>)>,
    pub(crate) published: Vec<super::FakeService>,
    /// Mod events the mod defined: name and layout, by id.
    pub(crate) defined_events: Vec<(String, Vec<(String, u64)>)>,
    /// Mod event handlers: name, layout, handler, pointer.
    pub(crate) mod_event_subscriptions: Vec<ModEventSubscription>,
    /// The names of the mod events the mod sent, in order.
    pub(crate) sent_events: Vec<String>,
    /// The migrations the mod declared.
    pub(crate) migrations: Option<super::FakeMigrations>,
    /// Content ids the mod declared, in order.
    pub(crate) content_ids: Vec<String>,
    pub(crate) defaults: Vec<String>,
    pub(crate) synced: Vec<String>,
    /// The mod's player actions callback, once registered.
    pub(crate) actions: Option<(crate::abi::ActionFn, usize)>,
    pub(crate) chat: Vec<(u64, String)>,
    pub(crate) labels: Vec<(u32, String)>,
    pub(crate) rename_filters: Vec<(RenameCallFn, usize)>,
    pub(crate) inventory_requests: Vec<OpenRequest>,
    pub(crate) status_sources: Vec<(StatusFn, usize)>,
    /// The mod's features as it last reported them: name and, when off,
    /// the reason.
    pub(crate) features: Vec<(String, Option<String>)>,
    /// The lines the mod ran on the console, in order.
    pub(crate) console: Vec<String>,
    /// The entities the mod asked to destroy, in order.
    pub(crate) destroyed: Vec<u32>,
    /// How many saves the mod asked for.
    pub(crate) saves: usize,
    /// The components the mod asked to write, `(entity, type hash)`.
    pub(crate) writes: Vec<(u32, u32)>,
    /// The game events the mod raised: type hash and bytes.
    pub(crate) emits: Vec<(u32, Vec<u8>)>,
    /// The teleports the mod asked for.
    pub(crate) teleports: Vec<TeleportRequest>,
    /// The spawns the mod asked for, with the ids handed out.
    pub(crate) spawns: Vec<(u32, SpawnRequest)>,
    pub(crate) craft_filters: Vec<(CraftCheckFn, usize)>,
    /// Channels the mod opened: full name, id, handler and pointer.
    pub(crate) channels: Vec<(String, u32, NetMessageFn, usize)>,
    /// Messages the mod sent: channel id, raw peer and bytes.
    pub(crate) net_sent: Vec<(u32, u64, Vec<u8>)>,
    /// Notices the mod showed: account key and text.
    pub(crate) notices: Vec<(u64, String)>,
    pub(crate) build_filters: Vec<(BuildCheckFn, usize)>,
    /// The clock `time_of_day` writes, settable while installed.
    pub(crate) daytime: Option<DaytimeInfo>,
    /// The mod's factor on the clock's speed, once it asked to change it.
    pub(crate) clock_speed: Option<f32>,
    /// The game settings' bytes and preset, settable while installed.
    pub(crate) game_settings: Option<(Vec<u8>, u32)>,
    /// The game settings' bytes the mod kept with the world, by offset.
    pub(crate) kept_settings: Vec<Option<u8>>,
    pub(crate) damage_filters: Vec<(DamageFilterFn, usize)>,
    pub(crate) knowledge_input_filters: Vec<(KnowledgeInputFn, usize)>,
    /// The buff types the fake knows.
    pub(crate) buff_types: Vec<BuffTypeInfo>,
    /// The buffs of each entity that holds buffs.
    pub(crate) buffs: std::collections::HashMap<u32, Vec<ActiveBuff>>,
    /// Knowledge values by `(character or 0, id)`.
    pub(crate) knowledge: Vec<(u32, KnowledgeInfo)>,
    /// The grants the mod asked for: character, id, kind, value.
    pub(crate) grants: Vec<(u32, u32, u32, u32)>,
    /// The revokes the mod asked for: character, id, kind, value.
    pub(crate) revokes: Vec<(u32, u32, u32, u32)>,
    /// The changes the mod asked for accounts: key, id, kind, value,
    /// whether a revoke.
    pub(crate) account_changes: Vec<(u64, u32, u32, u32, bool)>,
    /// The quest ids the fake knows.
    pub(crate) quests: Vec<u32>,
    /// The weather weights everywhere and the fronts, settable.
    pub(crate) weather: Option<([f32; 4], Vec<WeatherFront>)>,
    /// The weather the mod started: effect and duration.
    pub(crate) forecasts: Vec<(u32, i64)>,
    /// The terrain materials, once a world runs.
    pub(crate) terrain: Option<Vec<TerrainMaterialInfo>>,
    /// The terrain edits the mod asked for.
    pub(crate) terrain_edits: Vec<TerrainEdit>,
    pub(crate) config_schema: Option<String>,
    /// Frame callbacks: callback and pointer.
    pub(crate) frames: Vec<(UiFrameFn, usize)>,
    /// HUD callbacks: callback and pointer.
    pub(crate) huds: Vec<(UiFrameFn, usize)>,
    /// What the HUD drawing now drew, with each item's anchor, while one
    /// draws.
    pub(crate) hud: Option<(u64, Vec<(u32, super::Drawn)>)>,
    /// The mod's markers on the map.
    pub(crate) markers: Vec<super::FakeMarker>,
    /// The mod's check of new settings texts.
    pub(crate) config_check: Option<(ConfigCheckFn, usize)>,
    /// The texts the mod handed the loader last, as TOML.
    pub(crate) published_texts: Option<String>,
    /// The mod's answer to an update pick.
    /// The frame drawing now: its token, whether a window is open and what
    /// was drawn.
    pub(crate) frame: Option<(u64, bool, Vec<super::Drawn>)>,
    /// Inputs waiting for the widget with their text.
    pub(crate) inputs: Vec<(String, super::Input)>,
    /// Dialogs waiting for an answer, with where it goes.
    pub(crate) dialogs: Vec<(super::ShownDialog, UiAnswerFn, usize)>,
    /// The world key last handed out, alive until the next one.
    world_key: Option<Box<str>>,
    /// The altars `altars` lists, settable while installed.
    pub(crate) altars: Vec<AltarInfo>,
    /// Whether the mod opened altars.
    pub(crate) altars_opened: bool,
    /// Whether the mod sizes the altar zones.
    pub(crate) zone_control: bool,
    /// The zone sizes the mod set last.
    pub(crate) zone_sizes: Vec<AltarZoneSize>,
    pub(crate) zone_filters: Vec<(ZoneCheckFn, usize)>,
    pub(crate) survivor_lookup: Option<(SurvivorLookupFn, usize)>,
    pub(crate) bed_checks: Vec<(BedCheckFn, usize)>,
    pub(crate) summons: Vec<(SummonFn, usize)>,
    pub(crate) summoned: Vec<(SummonFn, usize)>,
    /// Item data kinds by full id, and values by kind and entity: the item
    /// and the bytes.
    pub(crate) item_kinds: Vec<String>,
    /// Content served or put in the fake archive, by hash.
    pub(crate) content: std::collections::BTreeMap<[u32; 4], Vec<u8>>,
    pub(crate) item_values: std::collections::BTreeMap<(u32, u32), (u32, Vec<u8>)>,
    pub(crate) summon_items: Vec<(u32, u32)>,
    /// Stat kinds, modifiers and worn items.
    pub(crate) stats: super::stats::FakeStats,
    /// Extension points the mod defined and links it attached.
    pub(crate) points: super::points::FakePoints,
    pub(crate) survivor_status: Option<(SurvivorStatusFn, usize)>,
    pub(crate) interaction_filters: Vec<(InteractionFn, usize)>,
    pub(crate) skills: Vec<(u32, SkillCheckFn, usize)>,
    pub(crate) comfort_filters: Vec<(ComfortFn, usize)>,
    pub(crate) experience_filters: Vec<(ExperiencePassFn, usize)>,
    pub(crate) altar_limit: Option<(AltarLimitFn, usize)>,
    pub(crate) base_limit_raised: bool,
    pub(crate) grouping: Option<(u32, u32)>,
    /// The answer `run_console` last handed out, alive until the next call.
    run_out: Option<Box<str>>,
    /// The settings texts handed out, alive while the fake is installed.
    settings: Vec<Box<str>>,
    /// The join values handed out, alive while the fake is installed.
    sync_out: Vec<Box<str>>,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);
/// Held by the installed fake, so one test at a time owns the table.
static TURN: Mutex<()> = Mutex::new(());

pub(crate) fn state() -> MutexGuard<'static, Option<State>> {
    STATE.relock()
}

thread_local! {
    /// Whether this thread holds the installed fake.
    static HOLDS_TURN: core::cell::Cell<bool> = const { core::cell::Cell::new(false) };
}

/// Waits for the table to be free and takes it for this thread.
///
/// # Panics
///
/// When this thread holds an installed fake already, which waiting would
/// never release.
fn take_turn() -> MutexGuard<'static, ()> {
    assert!(
        !HOLDS_TURN.get(),
        "a fake host is already installed in this thread; drop the first `Installed` before the next install"
    );
    let turn = TURN.relock();
    HOLDS_TURN.set(true);
    turn
}

/// Waits for the table to be free, then answers from `host`.
pub(crate) fn install(host: FakeHost) -> MutexGuard<'static, ()> {
    let turn = take_turn();
    *state() = Some(State {
        host,
        logs: Vec::new(),
        faults: Vec::new(),
        hooks: Vec::new(),
        mid_hooks: Vec::new(),
        subscriptions: Vec::new(),
        game_events: Vec::new(),
        served: Vec::new(),
        rebuilds: Vec::new(),
        recipe_rules: Vec::new(),
        texts: Vec::new(),
        code_patches: Vec::new(),
        published: Vec::new(),
        defined_events: Vec::new(),
        mod_event_subscriptions: Vec::new(),
        sent_events: Vec::new(),
        migrations: None,
        content_ids: Vec::new(),
        defaults: Vec::new(),
        synced: Vec::new(),
        inventory_requests: Vec::new(),
        sync_out: Vec::new(),
        actions: None,
        chat: Vec::new(),
        labels: Vec::new(),
        rename_filters: Vec::new(),
        status_sources: Vec::new(),
        features: Vec::new(),
        console: Vec::new(),
        destroyed: Vec::new(),
        saves: 0,
        writes: Vec::new(),
        emits: Vec::new(),
        teleports: Vec::new(),
        spawns: Vec::new(),
        craft_filters: Vec::new(),
        daytime: None,
        clock_speed: None,
        game_settings: None,
        kept_settings: Vec::new(),
        damage_filters: Vec::new(),
        knowledge_input_filters: Vec::new(),
        buff_types: Vec::new(),
        buffs: std::collections::HashMap::new(),
        knowledge: Vec::new(),
        grants: Vec::new(),
        revokes: Vec::new(),
        account_changes: Vec::new(),
        quests: Vec::new(),
        weather: None,
        forecasts: Vec::new(),
        terrain: None,
        terrain_edits: Vec::new(),
        config_schema: None,
        channels: Vec::new(),
        net_sent: Vec::new(),
        notices: Vec::new(),
        frames: Vec::new(),
        huds: Vec::new(),
        hud: None,
        markers: Vec::new(),
        config_check: None,
        published_texts: None,
        frame: None,
        inputs: Vec::new(),
        dialogs: Vec::new(),
        build_filters: Vec::new(),
        world_key: None,
        altars: Vec::new(),
        altars_opened: false,
        zone_control: false,
        zone_sizes: Vec::new(),
        zone_filters: Vec::new(),
        survivor_lookup: None,
        bed_checks: Vec::new(),
        summons: Vec::new(),
        summoned: Vec::new(),
        item_kinds: Vec::new(),
        content: std::collections::BTreeMap::new(),
        item_values: std::collections::BTreeMap::new(),
        summon_items: Vec::new(),
        stats: super::stats::FakeStats::default(),
        points: super::points::FakePoints::default(),
        survivor_status: None,
        interaction_filters: Vec::new(),
        skills: Vec::new(),
        comfort_filters: Vec::new(),
        experience_filters: Vec::new(),
        altar_limit: None,
        base_limit_raised: false,
        grouping: None,
        run_out: None,
        settings: Vec::new(),
    });
    turn
}

pub(crate) fn uninstall() {
    *state() = None;
    HOLDS_TURN.set(false);
}

/// `f` on the installed fake's state, or `default` when none is installed.
pub(crate) fn with<R>(default: R, f: impl FnOnce(&mut State) -> R) -> R {
    state().as_mut().map_or(default, f)
}

/// Writes `value` through `out`, which the safe layer passes valid or null.
pub(crate) fn put<T>(out: *mut T, value: T) -> Status {
    // SAFETY: the safe layer passes a valid out pointer; `as_mut` turns away null.
    match unsafe { out.as_mut() } {
        Some(slot) => {
            *slot = value;
            Status::OK
        }
        None => Status::BAD_ARGS,
    }
}

pub(crate) fn found<T>(value: Option<T>, out: *mut T) -> Status {
    value.map_or(Status::MISSING, |value| put(out, value))
}

pub(crate) fn text(s: Str) -> String {
    // SAFETY: strings are borrowed for the call.
    unsafe { s.as_str() }.map_or_else(|| "<not UTF-8>".to_owned(), str::to_owned)
}

/// Writes up to `capacity` of `items` to `out` and their number to `count`.
pub(crate) fn put_list<T: Copy>(
    items: &[T],
    out: *mut T,
    capacity: u32,
    count: *mut u32,
) -> Status {
    if count.is_null() {
        return Status::BAD_ARGS;
    }
    if !out.is_null() {
        for (i, item) in items.iter().take(capacity as usize).enumerate() {
            // SAFETY: `i` is below `capacity` and the safe layer passes a
            // non-null `out` with room for `capacity` entries, so the offset
            // stays inside that array.
            let slot = unsafe { out.add(i) };
            // SAFETY: `slot` is inside the `capacity` writable, aligned
            // entries the safe layer passes at `out`.
            unsafe { slot.write(*item) };
        }
    }
    // SAFETY: `count` is non-null, checked at the top; the safe layer passes
    // it pointing at a writable, aligned `u32` for the call.
    unsafe { count.write(u32::try_from(items.len()).unwrap_or(u32::MAX)) };
    Status::OK
}

/// The table every installed fake answers through.
pub(crate) static TABLE: HostApi = HostApi {
    header: Header::of::<HostApi>(),
    host_version: Str::new("0.0.0-test"),
    build,
    symbol,
    system,
    field_offset,
    type_size,
    hook,
    caller_frame,
    fault,
    log,
    settings,
    publish,
    service,
    constant,
    data_dir,
    log_level,
    subscribe,
    type_hash,
    component,
    entity_template,
    player,
    players,
    serve_resource,
    add_text,
    mods,
    sync,
    code_range,
    patch_code,
    workshop_recipes,
    hosting,
    server_world,
    retired_command,
    chat,
    set_label,
    rename_filter,
    join_missing,
    hook_at,
    inventory_open,
    item_info,
    inventory_submit,
    status,
    run_console,
    altars_open,
    altars,
    altar_zones_control,
    set_altar_zones,
    zone_check_filter,
    survivor_lookup,
    survivor_bed_check,
    summon_filter,
    summon_item,
    survivor_status,
    interaction_filter,
    skill_override,
    comfort_filter,
    experience_filter,
    altar_limit,
    raise_base_limit,
    base_zones,
    game_event,
    entities,
    destroy,
    save_world,
    world_key,
    component_write,
    emit,
    teleport,
    spawn,
    craft_filter,
    build_filter,
    net_channel,
    net_send,
    net_ready,
    notify,
    time_of_day,
    set_time_of_day,
    time_speed_control,
    set_time_speed,
    game_settings,
    set_game_settings,
    damage_filter,
    buff_types,
    buffs,
    apply_buff,
    remove_buff,
    knowledge,
    grant_knowledge,
    quests,
    weather_at,
    weather_fronts,
    set_weather,
    terrain_materials,
    terrain_edit,
    config_schema,
    summoned,
    item_data_open,
    item_data_get,
    item_data_set,
    ui_frame,
    ui_widget,
    ui_dialog,
    ui_hud,
    ui_markers,
    config_check,
    texts,
    content_serve,
    content_read,
    icons_ready,
    stat_open: super::stats::stat_open,
    stat_info: super::stats::stat_info,
    stat_value: super::stats::stat_value,
    stat_modify: super::stats::stat_modify,
    stat_buff: super::stats::stat_buff,
    stats_changed: super::stats::stats_changed,
    stat_sources: super::stats::stat_sources,
    stat_watch: super::stats::stat_watch,
    define_event,
    subscribe_event,
    send_event,
    affix_open: super::stats::affix_open,
    affix_roll: super::stats::affix_roll,
    item_lines: super::stats::item_lines,
    affix_info: super::stats::affix_info,
    migrate,
    declare_content,
    entity_data_open,
    replaced,
    knowledge_input_filter,
    language,
    keep_game_settings,
    forget_game_settings,
    revoke_knowledge,
    account_knowledge,
    resource_read,
    game_text,
    rebuild_resource,
    recipe_rule,
    recipe_disabled,
    define_point: super::points::define_point,
    attach_point: super::points::attach_point,
    run_point: super::points::run_point,
    actions,
    event_types,
    feature,
};
