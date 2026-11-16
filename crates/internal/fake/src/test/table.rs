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
    ActiveBuff, AltarInfo, AltarLimitFilterFn, BedCheckFn, BuffTypeInfo, BuildFilterFn,
    BuildingZoneFilterFn, ComfortFilterFn, ConfigCheckFn, CraftFilterFn, DamageFilterFn,
    DaytimeInfo, Event, EventFn, ExperienceFilterFn, GameEventFn, Header, HostApi,
    InteractionFilterFn, JoinRuleFn, KnowledgeInfo, KnowledgeInputFilterFn, Level, MidHookFn,
    ModEventFn, ModHandle, NetMessageFn, RenameFilterFn, ResourceKey, ResourcePatchFn,
    SkillCheckFn, SpawnRequest, StatusFn, Str, SummonFilterFn, SurvivorLookupFn, SurvivorStatusFn,
    TeleportRequest, TerrainMaterialInfo, UiAnswerFn, UiFrameFn, UiTooltipFn, WeatherFront,
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
    found, found_headed, get_headed, get_headed_list, list, put, put_headed, put_list, text,
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
pub(crate) type Extension = (
    String,
    u32,
    Registered<UiFrameFn>,
    Vec<enshwick_core::content::Guid>,
);

/// The local player's held controls as the test set them, `None` as outside
/// a game; whether a mod asked yet, as the loader starts reading them at the
/// first ask; and whether an interface step ran since, the first that reads
/// them.
pub(crate) struct Held {
    pub(crate) bits: Option<u64>,
    pub(crate) asked: bool,
    pub(crate) stepped: bool,
}

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
    /// Game event handlers of the client's own world, by type hash (0 for
    /// every type); none on a dedicated server.
    pub(crate) client_game_events: Vec<(u32, Registered<GameEventFn>)>,
    pub(crate) served: Vec<(ServedResource, Registered<ResourcePatchFn>)>,
    /// Every rebuild of a served resource the mod asked for, at init and
    /// after.
    pub(crate) rebuilds: Vec<ResourceKey>,
    /// How many buffers the host lent patch outputs and has not freed yet.
    pub(crate) lent_buffers: usize,
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
    pub(crate) content_renames:
        Option<Vec<(enshwick_core::content::Guid, enshwick_core::content::Guid)>>,
    /// Content ids the mod declared, in order.
    pub(crate) content_ids: Vec<String>,
    /// The crafting props the mod was given an index for, in order.
    pub(crate) crafting_props: Vec<enshwick_core::content::Guid>,
    /// The mod's player actions callback, once registered.
    pub(crate) actions: Option<Registered<enshwick_core::abi::ActionFn>>,
    pub(crate) chat: Vec<(enshwick_core::PlayerKey, String)>,
    pub(crate) labels: Vec<(u32, String)>,
    pub(crate) rename_filters: Vec<Registered<RenameFilterFn>>,
    pub(crate) join_rules: Vec<Registered<JoinRuleFn>>,
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
    pub(crate) craft_filters: Vec<Registered<CraftFilterFn>>,
    /// Channels the mod opened: full name, id, handler and pointer.
    pub(crate) channels: Vec<(String, u32, Registered<NetMessageFn>)>,
    /// Messages the mod sent: channel id, raw peer and bytes.
    pub(crate) net_sent: Vec<(u32, enshwick_core::PlayerKey, Vec<u8>)>,
    /// Notices the mod showed: account key and text.
    pub(crate) notices: Vec<(enshwick_core::PlayerKey, String)>,
    pub(crate) build_filters: Vec<Registered<BuildFilterFn>>,
    /// The clock `time_of_day` writes, settable while installed.
    pub(crate) daytime: Option<DaytimeInfo>,
    /// Each mod's factor on the clock's speed, once it asked to change it.
    pub(crate) clock_speeds: Vec<(ModHandle, f32)>,
    /// The game settings' bytes and preset, settable while installed.
    pub(crate) game_settings: Option<(Vec<u8>, u32)>,
    /// The game settings' bytes the mod kept with the world, by offset.
    pub(crate) kept_settings: Vec<Option<u8>>,
    pub(crate) damage_filters: Vec<Registered<DamageFilterFn>>,
    pub(crate) knowledge_input_filters: Vec<Registered<KnowledgeInputFilterFn>>,
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
    /// The terrain materials the mod added, in order.
    pub(crate) terrain_added: Vec<enshwick_core::abi::TerrainMaterialAdd>,
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
    pub(crate) screen_host_read: Option<(enshwick_core::content::Guid, [i64; 3])>,
    /// The game actions the fake build names, each with whether a key is
    /// bound to it.
    pub(crate) game_actions: Vec<(String, bool)>,
    /// The local player's held controls.
    pub(crate) local_held: Held,
    pub(crate) local_input_info: enshwick_core::abi::LocalInputInfo,
    /// The client snapshot the loader would write, as the test set it.
    pub(crate) client_snapshot: enshwick_core::abi::ClientSnapshot,
    /// Whether a snapshot ask started the frame's reads, and whether an
    /// interface step read them since.
    pub(crate) snapshot_reads: (bool, bool),
    /// The loader's parts and features off, as the test turned them off.
    pub(crate) loader_off: Vec<enshwick_core::abi::LoaderOff>,
    /// The client commands mods queued, each with its kind, target and
    /// status, as the test drives them.
    pub(crate) client_commands: Vec<(u64, u32, String, enshwick_core::abi::ClientCommandStatus)>,
    /// The slot of an inventory screen under the cursor, as the frames
    /// hear it: the packed slot, item, count and item entity.
    pub(crate) hovered: Option<(u64, enshwick_core::ItemId, u32, enshwick_core::Entity)>,
    /// Item tooltip callbacks.
    pub(crate) tooltips: Vec<Registered<UiTooltipFn>>,
    /// The lines the tooltip callback running now added, with its token.
    pub(crate) tooltip: Option<(u64, Vec<super::Drawn>)>,
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
    pub(crate) zone_filters: Vec<Registered<BuildingZoneFilterFn>>,
    /// Each mod's survivor lookup, in load order.
    pub(crate) survivor_lookup: Vec<Registered<SurvivorLookupFn>>,
    pub(crate) bed_checks: Vec<Registered<BedCheckFn>>,
    pub(crate) summons: Vec<Registered<SummonFilterFn>>,
    pub(crate) summoned: Vec<Registered<SummonFilterFn>>,
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
    /// Each mod's survivor menu merge, in load order.
    pub(crate) survivor_status: Vec<Registered<SurvivorStatusFn>>,
    pub(crate) interaction_filters: Vec<Registered<InteractionFilterFn>>,
    /// The prompts mods gave the objects placed from their items.
    pub(crate) interaction_prompts: Vec<enshwick_core::abi::InteractionPrompt>,
    pub(crate) item_filters: Vec<Registered<enshwick_core::abi::ItemFilterFn>>,
    pub(crate) chat_filters: Vec<Registered<enshwick_core::abi::ChatFilterFn>>,
    /// Each account's chat rate: the start of its window in fake server
    /// time and the lines heard in it.
    pub(crate) chat_rate: Vec<(enshwick_core::PlayerKey, u64, u32)>,
    /// The world's save on disk and the mod's hold on saves.
    pub(crate) save: Saves,
    pub(crate) skills: Vec<(enshwick_core::abi::SkillNode, Registered<SkillCheckFn>)>,
    pub(crate) comfort_filters: Vec<Registered<ComfortFilterFn>>,
    /// The rates the mod declared, with the factor it set for each.
    pub(crate) rates: Vec<(enshwick_core::abi::RateId, f32)>,
    /// What the mod asked of the engine's pacing places.
    pub(crate) pacing: Pacing,
    pub(crate) experience_filters: Vec<Registered<ExperienceFilterFn>>,
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
    /// The handles turned off, as the loader turns a mod off: its init or
    /// ready failed, or it faulted. None of their callbacks runs again.
    pub(crate) off: Vec<ModHandle>,
    /// The patches in `served` whose builder panicked, by place: each build
    /// runs without them, as the loader's.
    pub(crate) served_off: Vec<usize>,
    /// The mods' changes to players' permissions.
    pub(crate) permission_changes: permissions::Changes,
    /// The waits on transactions' ends, in the order asked.
    pub(crate) settle_waits: Vec<SettleWait>,
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
        client_game_events: Vec::new(),
        served: Vec::new(),
        rebuilds: Vec::new(),
        lent_buffers: 0,
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
        crafting_props: Vec::new(),
        inventory_requests: Vec::new(),
        actions: None,
        chat: Vec::new(),
        labels: Vec::new(),
        rename_filters: Vec::new(),
        join_rules: Vec::new(),
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
        clock_speeds: Vec::new(),
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
        terrain_added: Vec::new(),
        edits: WorldEdits::default(),
        fog_opened: false,
        config_schemas: std::collections::BTreeMap::new(),
        channels: Vec::new(),
        net_sent: Vec::new(),
        notices: Vec::new(),
        frames: Vec::new(),
        huds: Vec::new(),
        extensions: Vec::new(),
        hovered: None,
        tooltips: Vec::new(),
        tooltip: None,
        drawing: None,
        extending: None,
        open_screen: "",
        prompt: None,
        screen_host: 0,
        screen_host_read: None,
        game_actions: Vec::new(),
        local_held: Held {
            bits: None,
            asked: false,
            stepped: false,
        },
        local_input_info: enshwick_core::abi::LocalInputInfo {
            header: Header::of::<enshwick_core::abi::LocalInputInfo>(),
            flags: 0,
            reserved: 0,
            age_ms: u64::MAX,
        },
        client_snapshot: enshwick_core::abi::ClientSnapshot::EMPTY,
        snapshot_reads: (false, false),
        loader_off: Vec::new(),
        client_commands: Vec::new(),
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
        survivor_lookup: Vec::new(),
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
        survivor_status: Vec::new(),
        interaction_filters: Vec::new(),
        interaction_prompts: Vec::new(),
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
        off: Vec::new(),
        served_off: Vec::new(),
        permission_changes: permissions::Changes::default(),
        settle_waits: Vec::new(),
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

/// Whether the mod's init has returned, so what only init may ask for
/// comes too late, as the loader answers.
pub(crate) fn late() -> bool {
    with(false, |s| s.registration == Registration::Closed)
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
    mods,
    code_range,
    patch_code,
    workshop_recipes,
    hosting,
    server_world,
    join_missing,
    hook_at,
    status,
    subscribe_game_event,
    net_channel,
    net_send,
    net_ready,
    config_schema,
    config_check,
    texts,
    define_event,
    subscribe_mod_event,
    send_event,
    rename_content,
    mod_dir,
    language,
    define_point: super::base::points::define_point,
    attach_point: super::base::points::attach_point,
    run_point: super::base::points::run_point,
    event_types,
    feature,
    loop_wait,
    server_wait,
    task_workers,
    serve_call: super::base::calls::serve_call,
    find_call: super::base::calls::find_call,
    call: super::base::calls::call,
    subscribe_tick: super::base::ticks::subscribe_tick,
    set_tick_every: super::base::ticks::set_tick_every,
    set_config,
    subscribe_client_game_event,
    serve_resource,
    add_text,
    define_item_data,
    item_data_get,
    set_item_data,
    content_serve,
    content_read,
    icons_ready,
    declare_content,
    define_entity_data,
    resource_read,
    game_text,
    rebuild_resource,
    recipe_rule,
    recipe_disabled,
    recipe_kind,
    rate,
    set_rate,
    change_data,
    set_change,
    use_content,
    resource_keys,
    content_hashes,
    types,
    game_version,
    crafting_prop,
    read_only,
    resource_served,
    resource_buffer,
    content_name,
    player,
    players,
    chat,
    chat_all,
    set_label,
    rename_filter,
    item_info,
    request_altars,
    altars,
    building_zone_filter,
    base_zones,
    entities,
    destroy,
    save_world,
    world_key,
    component_write,
    notify,
    time_of_day,
    set_time_of_day,
    request_time_speed,
    set_time_speed,
    game_settings,
    set_game_settings,
    buff_types,
    buffs,
    apply_buff,
    remove_buff,
    weather_at,
    weather_fronts,
    set_weather,
    terrain_materials,
    terrain_edit,
    altar_zone_filter,
    altar_zones_changed,
    altar_limit_filter,
    base_limit,
    voxel_read,
    build_rights,
    transaction_commit,
    transaction_undo,
    transaction_status,
    world_quiet,
    transaction_entities,
    world_persists,
    map_markers,
    account_action,
    map_frame,
    request_base_limits,
    set_chat_mute,
    chat_muted,
    set_base_limit,
    save_info,
    hold_saves,
    set_saved,
    progress_write,
    server_stop,
    fog_uncover,
    language_of,
    join_rule,
    request_inventory,
    inventory_submit,
    survivor_lookup,
    survivor_bed_check,
    summon_filter,
    summon_item,
    survivor_status,
    emit,
    teleport,
    spawn,
    damage_filter,
    summoned,
    define_stat: super::inventory::stats::define_stat,
    stat_info: super::inventory::stats::stat_info,
    stat_value: super::inventory::stats::stat_value,
    stat_modify: super::inventory::stats::stat_modify,
    stat_buff: super::inventory::stats::stat_buff,
    stats_changed: super::inventory::stats::stats_changed,
    stat_sources: super::inventory::stats::stat_sources,
    stat_watch: super::inventory::stats::stat_watch,
    define_affix: super::inventory::stats::define_affix,
    affix_roll: super::inventory::stats::affix_roll,
    item_lines: super::inventory::stats::item_lines,
    affix_info: super::inventory::stats::affix_info,
    item_filter,
    drop_items,
    skill_override,
    comfort_filter,
    experience_filter,
    knowledge,
    change_knowledge,
    quests,
    knowledge_input_filter,
    game_query,
    run_console,
    interaction_filter,
    craft_filter,
    build_filter,
    actions,
    control,
    run_action,
    chat_filter,
    commands,
    run_command,
    command_list,
    actions_changed,
    action_window,
    ui_frame,
    ui_widget,
    ui_dialog,
    ui_hud,
    ui_markers,
    request_fog,
    fog_read,
    map_marker_types,
    ui_extension,
    local_held,
    local_input_status,
    client_snapshot,
    loader_off,
    client_command,
    client_command_status,
    terrain_material_add,
    interaction_prompt,
    ui_item_tooltip,
    change_permissions: permissions::change_permissions,
    transaction_settled,
};

/// What the loader runs after each step's own work: the waits it ended.
pub(crate) fn after_step() {
    run_settled();
}

/// What the loader forgets as a client's hosted game ends: the world's
/// transactions, after each wait on them ran.
pub(crate) fn end_game() {
    end_world();
}
