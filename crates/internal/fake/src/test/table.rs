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
use enshwick_core::abi::{Header, HostApi, Status, Str};

use super::FakeHost;

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
use base::{BaseState, build::*, hooks::*, host::*, mods::*, net::*, pacing::*, services::*};
use content::*;
use inventory::*;
use knowledge::*;
#[cfg(feature = "unstable")]
pub(crate) use ui::hear;
use ui::*;
use world::{altars::*, buffs::*, environment::*, saves::*, transactions::*, *};

/// What the installed fake answers and what it has recorded.
pub(crate) struct State {
    pub(crate) host: FakeHost,
    pub(crate) base: BaseState,
    pub(crate) content: ContentState,
    pub(crate) world: WorldState,
    pub(crate) inventory: InventoryState,
    pub(crate) knowledge: KnowledgeState,
    pub(crate) actions: ActionsState,
    pub(crate) ui: UiState,
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
    /// Calls this thread runs as the game's worker threads would.
    static ON_WORKER: core::cell::Cell<u32> = const { core::cell::Cell::new(0) };
}

/// Whether this thread installed the fake, the one that stands for the
/// server's game thread, outside a call [`on_worker`] runs.
pub(crate) fn on_game_thread() -> bool {
    HOLDS_TURN.get() && ON_WORKER.get() == 0
}

/// Runs `work` as the loader runs a filter on a game worker thread, where
/// [`on_game_thread`] answers false.
pub(crate) fn on_worker<R>(work: impl FnOnce() -> R) -> R {
    struct Leave;
    impl Drop for Leave {
        fn drop(&mut self) {
            ON_WORKER.set(ON_WORKER.get().wrapping_sub(1));
        }
    }
    ON_WORKER.set(ON_WORKER.get().wrapping_add(1));
    let _leave = Leave;
    work()
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
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Registration {
    /// The mod's init has not run yet.
    #[default]
    Open,
    /// The mod's init runs, before the loader has a world.
    Init,
    /// The mod's init returned; such requests come too late.
    Closed,
}

/// Waits for the table to be free, then answers from `host`.
pub(crate) fn install(mut host: FakeHost) -> MutexGuard<'static, ()> {
    // Before the turn: the folder is the install's own, and a failed write holds no turn.
    let mod_files = super::base::settings::place_mod_files(&mut host.base);
    let turn = take_turn();
    let mut base = BaseState::default();
    base.mod_files = mod_files;
    *state() = Some(State {
        host,
        base,
        content: ContentState::default(),
        world: WorldState::default(),
        inventory: InventoryState::default(),
        knowledge: KnowledgeState::default(),
        actions: ActionsState::default(),
        ui: UiState::default(),
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
    with(false, |s| s.base.closed())
}

/// `f` on the installed fake's state while only init may ask, as the
/// loader answers: [`Status::TOO_LATE`] once init returned,
/// [`Status::MISSING`] when no fake is installed.
pub(crate) fn with_open(f: impl FnOnce(&mut State) -> Status) -> Status {
    with(Status::MISSING, |s| {
        if s.base.closed() {
            Status::TOO_LATE
        } else {
            f(s)
        }
    })
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
    prop_placement_info,
    container_rule: containers::container_rule,
    show_pickup: containers::show_pickup,
    respawn_point: crate::test::world::respawn::respawn_point,
    set_respawn_point: crate::test::world::respawn::set_respawn_point,
    buff_type_add,
    damage_causes_off,
    waypoint: waypoints::waypoint,
    sound_play,
    sound_steer,
    sound_state,
    sound_play_for,
    sound_steer_for,
    rekey_resource,
};

/// What the loader runs after each step's own work: the waits it ended.
pub(crate) fn after_step() {
    run_settled();
    waypoints::after_step();
}

/// What the loader does as a client's hosted game ends: counts the ended
/// game and lets go of the world's save hold in one step, then forgets the
/// world's transactions, after each wait on them ran.
pub(crate) fn end_game() {
    with((), |s| {
        s.base.ticks.games_ended += 1;
        end_saves(s);
    });
    end_world();
}
