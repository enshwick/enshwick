//! The loader's table, answering from the installed fake.

use std::sync::{Mutex, MutexGuard};

use crate::abi::{
    ActiveBuff, AltarInfo, AltarLimitFilterFn, AltarZoneFn, BedCheckFn, BuffTypeInfo, BuildCheckFn,
    BuildingZoneCheckFn, ComfortFn, ConfigCheckFn, CraftCheckFn, DamageFilterFn, DaytimeInfo,
    Event, EventFn, ExperiencePassFn, GameEventFn, Header, HostApi, InteractionFn,
    InventoryResultFn, KnowledgeInfo, KnowledgeInputFn, Level, LoopWaitFn, MidHookFn, ModEventFn,
    NetMessageFn, RenameCallFn, ResourceKey, ResourcePatchFn, ServerWaitFn, SkillCheckFn,
    SpawnRequest, Status, StatusFn, Str, SummonFn, SurvivorLookupFn, SurvivorStatusFn,
    TeleportRequest, TerrainEdit, TerrainMaterialInfo, UiAnswerFn, UiFrameFn, WeatherFront,
};
use crate::locks::Relock;

use super::{FakeHost, LinkedHook, ServedResource, SubmittedRequest};

mod build;
mod content;
pub(crate) mod gameplay;
mod hooks;
mod inventory;
mod mods;
mod net;
mod pacing;
mod services;
mod transactions;
mod ui;
pub(crate) mod world;

use build::{
    build, caller_frame, constant, data_dir, field_offset, log_level, symbol, system, type_hash,
    type_size,
};
use content::{
    add_text, change_data, content_read, content_serve, declare_change, entity_data_open,
    game_text, icons_ready, item_data_get, item_data_open, item_data_set, rebuild_resource,
    recipe_disabled, recipe_kind, recipe_rule, resource_read, serve_resource, set_change,
};
use gameplay::{
    account_knowledge, altar_limit_filter, altar_zone_filter, altar_zones_changed, altars,
    altars_open, apply_buff, base_limit, base_zones, buff_types, buffs, build_filter,
    building_zone_check_filter, comfort_filter, craft_filter, damage_filter, experience_filter,
    grant_knowledge, interaction_filter, knowledge, knowledge_input_filter, quests,
    raise_base_limit, rate, remove_buff, request_base_limits, revoke_knowledge, set_rate,
    skill_override, summon_filter, summon_item, summoned, survivor_bed_check, survivor_lookup,
    survivor_status,
};
use hooks::{code_range, hook, hook_at, patch_code, workshop_recipes};
use inventory::{inventory_open, inventory_submit, item_info};
use mods::{
    config_check, config_schema, fault, join_missing, language, log, migrate, mods, replaced,
    settings, sync, texts,
};
use net::{net_channel, net_ready, net_send};
pub(crate) use services::view;
use services::{
    declare_content, define_event, publish, send_event, service, subscribe, subscribe_event,
};
use transactions::{
    transaction_commit, transaction_commit_for, transaction_entities, transaction_status,
    transaction_undo,
};
use ui::{fog_open, fog_read, ui_dialog, ui_frame, ui_hud, ui_markers, ui_widget};
use world::{
    account_action, actions, allowlist_has, allowlist_set, build_rights, chat, component,
    component_write, destroy, emit, entities, entity_template, event_types, feature,
    forget_game_settings, game_event, game_settings, hosting, keep_game_settings, map_frame,
    map_markers, notify, player, players, rename_filter, run_console, save_world, server_world,
    set_game_settings, set_label, set_time_of_day, set_time_speed, set_weather, spawn, status,
    teleport, terrain_edit, terrain_materials, time_of_day, time_speed_control, voxel_read,
    weather_at, weather_fronts, world_key, world_persists, world_quiet,
};

/// A submitted inventory request and where its result goes.
pub(crate) struct OpenRequest {
    pub(crate) request: SubmittedRequest,
    pub(crate) done: InventoryResultFn,
    pub(crate) user: usize,
}

/// A defined mod event: name, layout and the size of its data.
pub(crate) type DefinedEvent = (String, Vec<(String, u64)>, u32);

/// A mod event handler: name, layout, the bytes it reads, handler and the
/// mod's pointer.
pub(crate) type ModEventSubscription = (String, Vec<(String, u64)>, u32, ModEventFn, usize);

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
    /// The changes of vanilla values the mod declared, with their sources.
    pub(crate) changes: DataChanges,
    /// The mod's recipe rules in force: kind and name.
    pub(crate) recipe_rules: Vec<(crate::abi::RecipeRuleKind, String)>,
    pub(crate) texts: Vec<(u32, u32, String)>,
    pub(crate) code_patches: Vec<(u64, Vec<u8>, Vec<u8>)>,
    pub(crate) published: Vec<super::FakeService>,
    /// Mod events the mod defined: name and layout, by id.
    pub(crate) defined_events: Vec<DefinedEvent>,
    /// Mod event handlers: name, layout, handler, pointer.
    pub(crate) mod_event_subscriptions: Vec<ModEventSubscription>,
    /// The names of the mod events the mod sent, in order.
    pub(crate) sent_events: Vec<(String, Vec<u8>)>,
    /// The migrations the mod declared.
    pub(crate) migrations: Option<super::FakeMigrations>,
    /// Content ids the mod declared, in order.
    pub(crate) content_ids: Vec<String>,
    pub(crate) defaults: Vec<String>,
    pub(crate) synced: Vec<String>,
    /// The mod's player actions callback, once registered.
    pub(crate) actions: Option<(crate::abi::ActionFn, usize)>,
    pub(crate) chat: Vec<(crate::PlayerKey, String)>,
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
    /// The accounts on the fake server's ban list, in the order banned.
    pub(crate) banned: Vec<crate::PlayerKey>,
    /// The accounts the mod removed from the game, by kick or ban, in order.
    pub(crate) removed: Vec<u64>,
    /// The accounts on the fake server's allowlist, in the order listed.
    pub(crate) allowlisted: Vec<crate::PlayerKey>,
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
    pub(crate) net_sent: Vec<(u32, crate::PlayerKey, Vec<u8>)>,
    /// Notices the mod showed: account key and text.
    pub(crate) notices: Vec<(crate::PlayerKey, String)>,
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
    pub(crate) knowledge: Vec<(Option<crate::Entity>, KnowledgeInfo)>,
    /// The grants the mod asked for: character, id, kind, value.
    pub(crate) grants: Vec<(Option<crate::Entity>, crate::abi::KnowledgeId, u32, u32)>,
    /// The revokes the mod asked for: character, id, kind, value.
    pub(crate) revokes: Vec<(Option<crate::Entity>, crate::abi::KnowledgeId, u32, u32)>,
    /// The changes the mod asked for accounts: key, id, kind, value,
    /// whether a revoke.
    pub(crate) account_changes: Vec<(crate::PlayerKey, crate::abi::KnowledgeId, u32, u32, bool)>,
    /// The quest ids the fake knows.
    pub(crate) quests: Vec<crate::abi::KnowledgeId>,
    /// The weather weights everywhere and the fronts, settable.
    pub(crate) weather: Option<([f32; 4], Vec<WeatherFront>)>,
    /// The weather the mod started: effect and duration.
    pub(crate) forecasts: Vec<(u32, i64)>,
    /// The terrain materials, once a world runs.
    pub(crate) terrain: Option<Vec<TerrainMaterialInfo>>,
    /// The terrain edits and transactions the mod asked for.
    pub(crate) edits: WorldEdits,
    /// Whether the mod opened the fog of war.
    pub(crate) fog_opened: bool,
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
    /// The mod's altar zone filters, and how often it asked for a new sizing.
    pub(crate) zone_sizing: ZoneSizing,
    pub(crate) zone_filters: Vec<(BuildingZoneCheckFn, usize)>,
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
    pub(crate) summon_items: Vec<(crate::ItemId, crate::survivors::NpcId)>,
    /// Stat kinds, modifiers and worn items.
    pub(crate) stats: super::stats::FakeStats,
    /// Extension points the mod defined and links it attached.
    pub(crate) points: super::points::FakePoints,
    /// Calls the mod serves and asks for.
    pub(crate) calls: super::calls::FakeCalls,
    /// The fake steps and the mod's periodic tick handlers.
    pub(crate) ticks: super::ticks::FakeTicks,
    pub(crate) survivor_status: Option<(SurvivorStatusFn, usize)>,
    pub(crate) interaction_filters: Vec<(InteractionFn, usize)>,
    pub(crate) skills: Vec<(crate::abi::SkillNode, SkillCheckFn, usize)>,
    pub(crate) comfort_filters: Vec<(ComfortFn, usize)>,
    /// The rates the mod declared, with the factor it set for each.
    pub(crate) rates: Vec<(crate::abi::RateId, f32)>,
    /// What the mod asked of the engine's pacing places.
    pub(crate) pacing: Pacing,
    pub(crate) experience_filters: Vec<(ExperiencePassFn, usize)>,
    pub(crate) altar_limits: Vec<(AltarLimitFilterFn, usize)>,
    /// Whether the mod raised the base limit, and whether the hooks are
    /// armed, from which on the raised limit holds.
    pub(crate) base_limit: BaseLimitState,
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

/// Whether this thread installed the fake, the one that stands for the
/// server's game thread.
pub(crate) fn on_game_thread() -> bool {
    HOLDS_TURN.get()
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

/// The base limits the mod asked for, and whether the hooks are armed.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct BaseLimitState {
    /// The largest zones and bases the mod asked for, once it asked.
    pub(crate) requested: Option<(u32, u32)>,
    /// The mod's init returned and its hooks are armed.
    pub(crate) armed: bool,
}

/// The mod's altar zone filters, and how often it asked for a new sizing.
#[derive(Debug, Default)]
pub(crate) struct ZoneSizing {
    /// The filters and their pointers.
    pub(crate) filters: Vec<(AltarZoneFn, usize)>,
    /// How often the mod asked for the zones to be sized again.
    pub(crate) resizes: usize,
}

/// What the mod asked of the engine's pacing places.
#[derive(Debug, Default)]
pub(crate) struct Pacing {
    /// The loop wait handlers and their pointers.
    pub(crate) loop_waits: Vec<(LoopWaitFn, usize)>,
    /// The server wait handlers and their pointers.
    pub(crate) server_waits: Vec<(ServerWaitFn, usize)>,
    /// The lowest worker caps asked for, default and background.
    pub(crate) task_workers: Option<(u32, u32)>,
}

/// The changes of vanilla values a mod declared, each with its source.
#[derive(Debug, Default)]
pub(crate) struct DataChanges {
    /// Changes of one entry.
    pub(crate) one: Vec<(crate::content::changes::Change, String)>,
    /// Changes of every item a selection picks.
    pub(crate) items: Vec<(crate::content::changes::ItemsChange, String)>,
    /// Changes of every recipe a selection picks.
    pub(crate) recipes: Vec<(crate::content::changes::RecipesChange, String)>,
    /// Changes the mod sets later, numbered from 1 in this order, with
    /// the value in force.
    pub(crate) live: Vec<crate::test::content::LiveChange>,
}

/// The changes of the world the mod asked for, and its voxel reads, in
/// order.
#[derive(Default)]
pub(crate) struct WorldEdits {
    /// The voxel boxes the mod read on the game thread, `(min, size)`.
    pub(crate) voxel_reads: Vec<([u32; 3], [u32; 3])>,
    /// Direct terrain edits.
    pub(crate) terrain: Vec<TerrainEdit>,
    /// Committed transactions, oldest first.
    pub(crate) transactions: Vec<crate::test::FakeTransaction>,
    /// Whether the world has stopped, as between two worlds.
    pub(crate) stopped: bool,
    /// Whether the scene's map markers are held as still being read.
    pub(crate) markers_pending: bool,
    /// The reads of the scene the mod asked for in this world; the first
    /// ask of each starts it.
    pub(crate) scene_asked: Vec<SceneRead>,
}

/// A read of the scene's data the loader runs once per world.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SceneRead {
    /// The map markers.
    Markers,
    /// The map's frame.
    Frame,
}

impl WorldEdits {
    /// Notes that the mod asked for `read`; whether this was its first ask.
    pub(crate) fn first_ask(&mut self, read: SceneRead) -> bool {
        let first = !self.scene_asked.contains(&read);
        if first {
            self.scene_asked.push(read);
        }
        first
    }
}

/// Waits for the table to be free, then answers from `host`.
#[expect(
    clippy::too_many_lines,
    reason = "one line per part of the fake's state, which only grows"
)]
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
        changes: DataChanges::default(),
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
        banned: Vec::new(),
        removed: Vec::new(),
        allowlisted: Vec::new(),
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
        edits: WorldEdits::default(),
        fog_opened: false,
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
        zone_sizing: ZoneSizing::default(),
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
        calls: super::calls::FakeCalls::default(),
        ticks: super::ticks::FakeTicks::default(),
        survivor_status: None,
        interaction_filters: Vec::new(),
        skills: Vec::new(),
        comfort_filters: Vec::new(),
        rates: Vec::new(),
        pacing: Pacing::default(),
        experience_filters: Vec::new(),
        altar_limits: Vec::new(),
        base_limit: BaseLimitState::default(),
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
    building_zone_check_filter,
    survivor_lookup,
    survivor_bed_check,
    summon_filter,
    summon_item,
    survivor_status,
    interaction_filter,
    skill_override,
    comfort_filter,
    experience_filter,
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
    fog_open,
    fog_read,
    map_markers,
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
    recipe_kind,
    feature,
    rate,
    set_rate,
    change_data,
    loop_wait: pacing::loop_wait,
    server_wait: pacing::server_wait,
    task_workers: pacing::task_workers,
    altar_zone_filter,
    altar_zones_changed,
    altar_limit_filter,
    base_limit,
    voxel_read,
    declare_change,
    set_change,
    build_rights,
    transaction_commit,
    transaction_undo,
    transaction_status,
    transaction_commit_for,
    world_quiet,
    serve_call: super::calls::serve_call,
    find_call: super::calls::find_call,
    call: super::calls::call,
    transaction_entities,
    world_persists,
    account_action,
    allowlist_set,
    allowlist_has,
    subscribe_tick: super::ticks::subscribe_tick,
    set_tick_every: super::ticks::set_tick_every,
    map_frame,
    request_base_limits,
};
