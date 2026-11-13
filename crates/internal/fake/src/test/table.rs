//! The loader's table, answering from the installed fake: the one state
//! struct and the join of each domain's entries.
// Clippy leaves wildcard imports alone in a test build.
#![cfg_attr(
    not(test),
    expect(
        clippy::wildcard_imports,
        reason = "the join takes each domain's entries whole, so a new entry touches only its domain"
    )
)]

use std::sync::{Mutex, MutexGuard};

use enshwick_core::Relock;
use enshwick_core::abi::{
    ActiveBuff, AltarInfo, AltarLimitFilterFn, BedCheckFn, BuffTypeInfo, BuildCheckFn,
    BuildingZoneCheckFn, ComfortFn, ConfigCheckFn, CraftCheckFn, DamageFilterFn, DaytimeInfo,
    Event, EventFn, ExperiencePassFn, GameEventFn, Header, HostApi, InteractionFn, KnowledgeInfo,
    KnowledgeInputFn, Level, MidHookFn, ModEventFn, ModHandle, NetMessageFn, RenameCallFn,
    ResourceKey, ResourcePatchFn, SkillCheckFn, SpawnRequest, StatusFn, Str, SummonFn,
    SurvivorLookupFn, SurvivorStatusFn, TeleportRequest, TerrainMaterialInfo, UiAnswerFn,
    UiFrameFn, WeatherFront,
};

use super::{FakeHost, LinkedHook, ServedResource};

// Each domain's entries, lowest first, as `domains.toml` orders them.
pub(crate) mod actions;
pub(crate) mod base;
pub(super) mod content;
mod inventory;
mod knowledge;
mod ui;
pub(crate) mod world;

use actions::{commands::*, *};
pub(crate) use base::io::{
    found, found_headed, get_headed, get_headed_list, put, put_headed, put_headed_list, put_list,
    text,
};
pub(crate) use base::registered::Registered;
pub(crate) use base::services::view;
use base::{build::*, hooks::*, host::*, mods::*, net::*, pacing::*, services::*};
use content::*;
use inventory::*;
use knowledge::*;
use ui::*;
use world::{altars::*, buffs::*, environment::*, saves::*, transactions::*, *};

/// A defined mod event: name, layout and the size of its data.
pub(crate) type DefinedEvent = (String, Vec<(String, u64)>, u32);

/// A mod event handler: name, layout, the bytes it reads and the handler.
pub(crate) type ModEventSubscription = (String, Vec<(String, u64)>, u32, Registered<ModEventFn>);

/// An extension of a game screen: screen name, edge, callback and the host
/// templates it shows for (empty for every host).
pub(crate) type Extension = (String, u32, Registered<UiFrameFn>, Vec<[u8; 16]>);

/// What the installed fake answers and what it has recorded.
pub(crate) struct State {
    pub(crate) host: FakeHost,
    pub(crate) logs: Vec<(Level, String)>,
    pub(crate) faults: Vec<String>,
    /// The handle each log line and fault came under, in order.
    pub(crate) sent: Vec<(ModHandle, Level, String)>,
    pub(crate) hooks: Vec<LinkedHook>,
    /// The slots of the linked hooks: one slot joins one chain.
    pub(crate) hook_slots: Vec<usize>,
    /// The mod as its manifest names it, once its init ran.
    pub(crate) manifest: Option<ManifestFacts>,
    /// Whether the mod may still ask for what only init may.
    pub(crate) registration: Registration,
    /// Mid-function hooks: address, handler and the mod's pointer.
    pub(crate) mid_hooks: Vec<(u64, Registered<MidHookFn>)>,
    /// Loader event handlers, by event.
    pub(crate) subscriptions: Vec<(Event, Registered<EventFn>)>,
    /// Game event handlers, by type hash (0 for every type).
    pub(crate) game_events: Vec<(u32, Registered<GameEventFn>)>,
    pub(crate) served: Vec<(ServedResource, Registered<ResourcePatchFn>)>,
    /// Every rebuild of a served resource the mod asked for, at init and
    /// after.
    pub(crate) rebuilds: Vec<ResourceKey>,
    /// How many patch outputs were bytes of the patch's own, kept for the
    /// process, rather than a buffer the host lent.
    pub(crate) kept_outputs: usize,
    /// The changes of vanilla values the mod declared, with their sources.
    pub(crate) changes: DataChanges,
    /// The mod's recipe rules in force: kind and name.
    pub(crate) recipe_rules: Vec<(enshwick_core::abi::RecipeRuleKind, String)>,
    pub(crate) texts: Vec<(u32, u32, String)>,
    pub(crate) code_patches: Vec<(u64, Vec<u8>, Vec<u8>)>,
    pub(crate) published: Vec<super::FakeService>,
    /// Mod events the mod defined: name and layout, by id.
    pub(crate) defined_events: Vec<DefinedEvent>,
    /// Mod event handlers: name, layout, the bytes read and the handler.
    pub(crate) mod_event_subscriptions: Vec<ModEventSubscription>,
    /// The names of the mod events the mod sent, in order.
    pub(crate) sent_events: Vec<(String, Vec<u8>)>,
    /// The content renames the mod declared, old GUID to new.
    pub(crate) content_renames: Option<Vec<([u8; 16], [u8; 16])>>,
    /// Content ids the mod declared, in order.
    pub(crate) content_ids: Vec<String>,
    /// The crafting props the mod was given an index for, in order, after
    /// the game's count and the mask's bits of the first call.
    pub(crate) crafting_props: (Option<(u8, u8)>, Vec<[u8; 16]>),
    /// The mod's player actions callback, once registered.
    pub(crate) actions: Option<Registered<enshwick_core::abi::ActionFn>>,
    pub(crate) chat: Vec<(enshwick_core::PlayerKey, String)>,
    pub(crate) labels: Vec<(u32, String)>,
    pub(crate) rename_filters: Vec<Registered<RenameCallFn>>,
    pub(crate) inventory_requests: Vec<OpenRequest>,
    pub(crate) status_sources: Vec<Registered<StatusFn>>,
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
    pub(crate) banned: Vec<enshwick_core::PlayerKey>,
    /// The accounts the mod removed from the game, by kick or ban, in order.
    pub(crate) removed: Vec<u64>,
    /// The accounts muted in the fake server's chat, in the order muted.
    pub(crate) muted: Vec<enshwick_core::PlayerKey>,
    /// The components the mod asked to write, `(entity, type hash)`.
    pub(crate) writes: Vec<(u32, u32)>,
    /// The game events the mod raised: type hash and bytes.
    pub(crate) emits: Vec<(u32, Vec<u8>)>,
    /// The teleports the mod asked for.
    pub(crate) teleports: Vec<TeleportRequest>,
    /// The spawns the mod asked for, with the ids handed out.
    pub(crate) spawns: Vec<(u32, SpawnRequest)>,
    /// The step each of `spawns` was asked in: the entity is in the world
    /// from the next one, as the loader's spawns are.
    pub(crate) spawn_steps: Vec<u64>,
    /// The level and experience writes the mod asked for: character, what
    /// and value; `None` until it asks for them in `init`.
    pub(crate) progress_writes: Option<Vec<(enshwick_core::Entity, u32, u32)>>,
    /// The step each of `progress_writes` was asked in.
    pub(crate) progress_steps: Vec<u64>,
    /// The drops the mod asked for, with the ids handed out.
    pub(crate) drops: Vec<(enshwick_core::Entity, enshwick_core::abi::DropRequest)>,
    /// The server stops the mod asked for and the fake took: 0 or 1.
    pub(crate) stops: u8,
    /// The fog uncovers the mod sent: player, centre and radius.
    pub(crate) uncovers: Vec<(enshwick_core::PlayerKey, [f32; 2], f32)>,
    pub(crate) craft_filters: Vec<Registered<CraftCheckFn>>,
    /// Channels the mod opened: full name, id, handler and pointer.
    pub(crate) channels: Vec<(String, u32, Registered<NetMessageFn>)>,
    /// Messages the mod sent: channel id, raw peer and bytes.
    pub(crate) net_sent: Vec<(u32, enshwick_core::PlayerKey, Vec<u8>)>,
    /// Notices the mod showed: account key and text.
    pub(crate) notices: Vec<(enshwick_core::PlayerKey, String)>,
    pub(crate) build_filters: Vec<Registered<BuildCheckFn>>,
    /// The clock `time_of_day` writes, settable while installed.
    pub(crate) daytime: Option<DaytimeInfo>,
    /// The mod's factor on the clock's speed, once it asked to change it.
    pub(crate) clock_speed: Option<f32>,
    /// The game settings' bytes and preset, settable while installed.
    pub(crate) game_settings: Option<(Vec<u8>, u32)>,
    /// The game settings' bytes the mod kept with the world, by offset.
    pub(crate) kept_settings: Vec<Option<u8>>,
    pub(crate) damage_filters: Vec<Registered<DamageFilterFn>>,
    pub(crate) knowledge_input_filters: Vec<Registered<KnowledgeInputFn>>,
    /// The buff types the fake knows.
    pub(crate) buff_types: Vec<BuffTypeInfo>,
    /// The buffs of each entity that holds buffs.
    pub(crate) buffs: rustc_hash::FxHashMap<u32, Vec<ActiveBuff>>,
    /// Knowledge values by `(character or 0, id)`.
    pub(crate) knowledge: Vec<(Option<enshwick_core::Entity>, KnowledgeInfo)>,
    /// Whether each game query holds, by `(character or 0, query)`.
    pub(crate) game_queries: Vec<(Option<enshwick_core::Entity>, u32, bool)>,
    /// The grants the mod asked for: character, id, kind, value.
    pub(crate) grants: Vec<(
        Option<enshwick_core::Entity>,
        enshwick_core::abi::KnowledgeId,
        u32,
        u32,
    )>,
    /// The revokes the mod asked for: character, id, kind, value.
    pub(crate) revokes: Vec<(
        Option<enshwick_core::Entity>,
        enshwick_core::abi::KnowledgeId,
        u32,
        u32,
    )>,
    /// The changes the mod asked for accounts: key, id, kind, value,
    /// whether a revoke.
    pub(crate) account_changes: Vec<(
        enshwick_core::PlayerKey,
        enshwick_core::abi::KnowledgeId,
        u32,
        u32,
        bool,
    )>,
    /// The quest ids the fake knows.
    pub(crate) quests: Vec<enshwick_core::abi::KnowledgeId>,
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
    /// The schemas of the mod's settings files, by name, as TOML.
    pub(crate) config_schemas: std::collections::BTreeMap<String, String>,
    /// Frame callbacks.
    pub(crate) frames: Vec<Registered<UiFrameFn>>,
    /// HUD callbacks.
    pub(crate) huds: Vec<Registered<UiFrameFn>>,
    /// Extensions of game screens: screen name, edge, callback and the host
    /// templates it shows for (empty for every host).
    pub(crate) extensions: Vec<Extension>,
    /// The handle whose callback draws now: the loader refuses a widget
    /// from any other.
    pub(crate) drawing: Option<ModHandle>,
    /// The frame token of the extension drawing now, whose window the
    /// loader opened.
    pub(crate) extending: Option<u64>,
    /// The game screen the player has open, as frames and HUDs hear it;
    /// empty for none.
    pub(crate) open_screen: &'static str,
    /// The interaction prompt the frames hear: its title and verb.
    pub(crate) prompt: Option<(String, u32)>,
    /// The entity the open screen belongs to, 0 for none.
    pub(crate) screen_host: u32,
    /// The host's template and fixed-point position as the client's last
    /// step read them, `None` before one did.
    pub(crate) screen_host_read: Option<([u8; 16], [i64; 3])>,
    /// The game actions the fake build names, each with whether a key is
    /// bound to it.
    pub(crate) game_actions: Vec<(String, bool)>,
    /// The local player's held controls, `None` as outside a game.
    pub(crate) local_held: Option<u64>,
    /// What the HUD drawing now drew, with each item's anchor, while one
    /// draws.
    pub(crate) hud: Option<(u64, Vec<(u32, super::Drawn)>)>,
    /// The mod's markers on the map.
    pub(crate) markers: Vec<super::FakeMarker>,
    /// The mod's checks of new settings texts, by file.
    pub(crate) config_checks: std::collections::BTreeMap<String, Registered<ConfigCheckFn>>,
    /// The mod's settings writes in flight, one per nested write, each
    /// `true` while it runs the mod's check, as the loader refuses them.
    pub(crate) config_writes: Vec<bool>,
    /// The texts the mod handed the loader last, as TOML.
    pub(crate) published_texts: Option<String>,
    /// The frame drawing now: its token, whether a window is open and what
    /// was drawn.
    pub(crate) frame: Option<(u64, bool, Vec<super::Drawn>)>,
    /// Inputs waiting for the widget with their text.
    pub(crate) inputs: Vec<(String, super::Input)>,
    /// Dialogs waiting for an answer, with where it goes.
    pub(crate) dialogs: Vec<(super::ShownDialog, Registered<UiAnswerFn>)>,
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
    pub(crate) zone_filters: Vec<Registered<BuildingZoneCheckFn>>,
    pub(crate) survivor_lookup: Option<Registered<SurvivorLookupFn>>,
    pub(crate) bed_checks: Vec<Registered<BedCheckFn>>,
    pub(crate) summons: Vec<Registered<SummonFn>>,
    pub(crate) summoned: Vec<Registered<SummonFn>>,
    /// Item data kinds by full id, and values by kind and entity: the item
    /// and the bytes.
    pub(crate) item_kinds: Vec<String>,
    /// Content served or put in the fake archive, by hash.
    pub(crate) content: std::collections::BTreeMap<[u32; 4], Vec<u8>>,
    pub(crate) item_values: std::collections::BTreeMap<(u32, u32), (u32, Vec<u8>)>,
    pub(crate) summon_items: Vec<(enshwick_core::ItemId, enshwick_core::survivors::NpcId)>,
    /// Stat kinds, modifiers and worn items.
    pub(crate) stats: super::inventory::stats::FakeStats,
    /// Extension points the mod defined and links it attached.
    pub(crate) points: super::base::points::FakePoints,
    /// Calls the mod serves and asks for.
    pub(crate) calls: super::base::calls::FakeCalls,
    /// The fake steps and the mod's periodic tick handlers.
    pub(crate) ticks: super::base::ticks::FakeTicks,
    /// The mod's controls and the action runs it sent.
    #[cfg(feature = "unstable")]
    pub(crate) controls: super::actions::controls::FakeControls,
    pub(crate) survivor_status: Option<Registered<SurvivorStatusFn>>,
    pub(crate) interaction_filters: Vec<Registered<InteractionFn>>,
    pub(crate) item_filters: Vec<Registered<enshwick_core::abi::ItemActionFn>>,
    pub(crate) chat_filters: Vec<Registered<enshwick_core::abi::ChatFilterFn>>,
    /// Each account's chat rate: the start of its window in fake server
    /// time and the lines heard in it.
    pub(crate) chat_rate: Vec<(enshwick_core::PlayerKey, u64, u32)>,
    /// The world's save on disk and the mod's hold on saves.
    pub(crate) save: Saves,
    pub(crate) skills: Vec<(enshwick_core::abi::SkillNode, Registered<SkillCheckFn>)>,
    pub(crate) comfort_filters: Vec<Registered<ComfortFn>>,
    /// The rates the mod declared, with the factor it set for each.
    pub(crate) rates: Vec<(enshwick_core::abi::RateId, f32)>,
    /// What the mod asked of the engine's pacing places.
    pub(crate) pacing: Pacing,
    pub(crate) experience_filters: Vec<Registered<ExperiencePassFn>>,
    pub(crate) altar_limits: Vec<Registered<AltarLimitFilterFn>>,
    /// Whether the mod raised the base limit, and whether the hooks are
    /// armed, from which on the raised limit holds.
    pub(crate) base_limit: BaseLimitState,
    pub(crate) grouping: Option<(u32, u32)>,
    /// The command name and alias the mod took, and how often it said
    /// its rules changed.
    pub(crate) commands: FakeCommands,
    /// The answer `run_console` last handed out, alive until the next call.
    run_out: Option<Box<str>>,
    /// The settings texts handed out, alive while the fake is installed.
    settings: Vec<Box<str>>,
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

/// Whether the mod may still ask for hooks, code patches, points and rates:
/// the loader takes them during init only and arms once every mod is
/// initialised.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Registration {
    /// The mod's init runs, or has not run yet.
    Open,
    /// The mod's init returned; such requests come too late.
    Closed,
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
        sent: Vec::new(),
        hooks: Vec::new(),
        hook_slots: Vec::new(),
        manifest: None,
        registration: Registration::Open,
        mid_hooks: Vec::new(),
        subscriptions: Vec::new(),
        game_events: Vec::new(),
        served: Vec::new(),
        rebuilds: Vec::new(),
        kept_outputs: 0,
        changes: DataChanges::default(),
        recipe_rules: Vec::new(),
        texts: Vec::new(),
        code_patches: Vec::new(),
        published: Vec::new(),
        defined_events: Vec::new(),
        mod_event_subscriptions: Vec::new(),
        sent_events: Vec::new(),
        content_renames: None,
        content_ids: Vec::new(),
        crafting_props: (None, Vec::new()),
        inventory_requests: Vec::new(),
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
        muted: Vec::new(),
        writes: Vec::new(),
        emits: Vec::new(),
        teleports: Vec::new(),
        spawns: Vec::new(),
        spawn_steps: Vec::new(),
        progress_writes: None,
        progress_steps: Vec::new(),
        drops: Vec::new(),
        stops: 0,
        uncovers: Vec::new(),
        craft_filters: Vec::new(),
        daytime: None,
        clock_speed: None,
        game_settings: None,
        kept_settings: Vec::new(),
        damage_filters: Vec::new(),
        knowledge_input_filters: Vec::new(),
        buff_types: Vec::new(),
        buffs: rustc_hash::FxHashMap::default(),
        knowledge: Vec::new(),
        game_queries: Vec::new(),
        grants: Vec::new(),
        revokes: Vec::new(),
        account_changes: Vec::new(),
        quests: Vec::new(),
        weather: None,
        forecasts: Vec::new(),
        terrain: None,
        edits: WorldEdits::default(),
        fog_opened: false,
        config_schemas: std::collections::BTreeMap::new(),
        channels: Vec::new(),
        net_sent: Vec::new(),
        notices: Vec::new(),
        frames: Vec::new(),
        huds: Vec::new(),
        extensions: Vec::new(),
        drawing: None,
        extending: None,
        open_screen: "",
        prompt: None,
        screen_host: 0,
        screen_host_read: None,
        game_actions: Vec::new(),
        local_held: None,
        hud: None,
        markers: Vec::new(),
        config_checks: std::collections::BTreeMap::new(),
        config_writes: Vec::new(),
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
        stats: super::inventory::stats::FakeStats::default(),
        points: super::base::points::FakePoints::default(),
        calls: super::base::calls::FakeCalls::default(),
        ticks: super::base::ticks::FakeTicks::default(),
        #[cfg(feature = "unstable")]
        controls: super::actions::controls::FakeControls::default(),
        survivor_status: None,
        interaction_filters: Vec::new(),
        item_filters: Vec::new(),
        chat_filters: Vec::new(),
        chat_rate: Vec::new(),
        save: Saves {
            info: None,
            held: false,
            shut: false,
            unsaved: Vec::new(),
        },
        skills: Vec::new(),
        comfort_filters: Vec::new(),
        rates: Vec::new(),
        pacing: Pacing::default(),
        experience_filters: Vec::new(),
        altar_limits: Vec::new(),
        base_limit: BaseLimitState::default(),
        grouping: None,
        commands: FakeCommands::default(),
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
    stat_open: super::inventory::stats::stat_open,
    stat_info: super::inventory::stats::stat_info,
    stat_value: super::inventory::stats::stat_value,
    stat_modify: super::inventory::stats::stat_modify,
    stat_buff: super::inventory::stats::stat_buff,
    stats_changed: super::inventory::stats::stats_changed,
    stat_sources: super::inventory::stats::stat_sources,
    stat_watch: super::inventory::stats::stat_watch,
    define_event,
    subscribe_event,
    send_event,
    affix_open: super::inventory::stats::affix_open,
    affix_roll: super::inventory::stats::affix_roll,
    item_lines: super::inventory::stats::item_lines,
    affix_info: super::inventory::stats::affix_info,
    rename_content,
    declare_content,
    entity_data_open,
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
    define_point: super::base::points::define_point,
    attach_point: super::base::points::attach_point,
    run_point: super::base::points::run_point,
    actions,
    event_types,
    recipe_kind,
    feature,
    rate,
    set_rate,
    change_data,
    loop_wait,
    server_wait,
    task_workers,
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
    serve_call: super::base::calls::serve_call,
    find_call: super::base::calls::find_call,
    call: super::base::calls::call,
    transaction_entities,
    world_persists,
    account_action,
    subscribe_tick: super::base::ticks::subscribe_tick,
    set_tick_every: super::base::ticks::set_tick_every,
    map_frame,
    request_base_limits,
    map_marker_types,
    use_content,
    chat_mute_set,
    chat_mute_has,
    mod_dir,
    set_base_cap,
    resource_keys,
    content_hashes,
    types,
    game_version,
    control,
    run_action,
    ui_extension,
    item_filter,
    chat_filter,
    save_info,
    hold_saves,
    crafting_prop,
    set_saved,
    run_action_at,
    local_held,
    read_only,
    resource_served,
    commands,
    run_command,
    command_list,
    actions_changed,
    resource_buffer,
    game_query,
    action_window,
    progress_write,
    drop_items,
    config_set,
    server_stop,
    fog_uncover,
    content_name,
};
