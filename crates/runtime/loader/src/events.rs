//! Game events on the server, raised from the loader's own links: tick, world
//! loaded and saved, player joined and left, shutdown, building placed and
//! removed. The post-step link is always made, since it also tells the world
//! reads which thread is the game's, and so is the state link, which loads
//! each world's settings and record; the others only when a mod subscribed to
//! one of their events.
//!
//! A client runs the same server code on its `GameServer` thread while it
//! hosts a game, solo play included, and only then: the links fire for that
//! built-in server as they do on the dedicated one. Each hosted game has its
//! own server and thread; the client follows them as a [`Session`] and, when a
//! game stops, forgets its server, so the next one starts clean.
//!
//! Building happens inside ECS systems on worker threads, so those links only
//! queue what happened; the post-step link raises it on the game thread.

use core::ffi::c_void;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use enshwick_core::abi::{BuildingData, Event, Header, HookFlags, PlayerData, Side, TickData};
use enshwick_core::raw::read_at;

use crate::engine::chains::Next;
use crate::engine::fuse::Fuse;
use crate::engine::loader::{Loader, Target};
use crate::engine::parts::{Part, PlayerCall, Stage, Stepping};
use crate::engine::pinned;
use crate::engine::pinned::{Constant, Symbol};
use crate::engine::ticks::{StepTime, TICKS};
use crate::{actions, world};
use enshwick_base::Relock;

/// The step's own part: its build table ids and its links, which the
/// domains' links rely on.
pub(crate) static PART: Part = Part {
    table_ids: TABLE_IDS,
    link: Some(link),
    ..Part::new("events", module_path!())
};

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[
    pinned::building::CTX_PLAYER.id(),
    pinned::building::CTX_REPLAY.id(),
    pinned::building::PICKUP.id(),
    pinned::building::PICKUP_ENTITY.id(),
    pinned::building::PICKUP_MODE.id(),
    pinned::building::SPAWN_PROP.id(),
    pinned::server::ENTRY_STATE.id(),
    pinned::server::PLAYER_REMOVE.id(),
    pinned::server::PLAYER_SLOT_COUNT.id(),
    pinned::server::PLAYER_SLOT_ENTITY.id(),
    pinned::server::PLAYER_SLOT_KEY.id(),
    pinned::server::PLAYER_SLOT_SIZE.id(),
    pinned::server::PLAYER_SLOT_STATE.id(),
    pinned::server::PLAYER_SLOTS.id(),
    pinned::server::POST_STEP.id(),
    pinned::server::SAVE_STEP.id(),
    pinned::server::SET_STATE.id(),
    pinned::server::WORLD.id(),
    pinned::world::ENTITIES.id(),
];

/// Server states, from the game's state machine. A hosted game enters
/// Initialize before its Load state reads the save (client 410 and 426).
const STATE_INITIALIZE: u8 = 2;
const STATE_RUN: u8 = 5;
const STATE_SHUTDOWN: u8 = 6;
/// The state whose updates tear the server down and free it, the
/// game-systems holder and the world, after `set_state` enters it (server
/// 399, client 426). The name is Enshwick's.
const STATE_TEAR_DOWN: u8 = 7;
use crate::engine::world::SLOT_IN_GAME;
/// Slots the joined set can hold; the game has 16.
const MAX_SLOTS: usize = 32;

/// Offsets into the game's server, from the build table.
#[derive(Debug, Clone, Copy)]
struct Offsets {
    entry_state: usize,
    slots: usize,
    slot_size: usize,
    slot_count: usize,
    slot_state: usize,
    slot_entity: usize,
    slot_key: usize,
    ctx_player: usize,
    ctx_replay: usize,
    /// The pickup target's entity id and mode byte.
    pickup_entity: usize,
    pickup_mode: usize,
    server_world: usize,
    entities: usize,
}

/// The server session the post-step link follows: the server and its thread.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Session {
    server: usize,
    thread: u32,
    /// Set when the session stopped; its last steps no longer count.
    ended: bool,
    /// The hosted games that ended, which a new session keeps.
    games_ended: u64,
}

/// What a step means for the session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Step {
    /// The session seen so far goes on.
    Same,
    /// Another server or thread: a new session.
    New,
    /// A step of a session that already stopped.
    Ended,
}

impl Session {
    /// Follows a step of `server` on `thread`.
    pub(crate) fn step(&mut self, server: usize, thread: u32) -> Step {
        if (server, thread) == (self.server, self.thread) {
            return if self.ended { Step::Ended } else { Step::Same };
        }
        *self = Self {
            server,
            thread,
            ended: false,
            games_ended: self.games_ended,
        };
        Step::New
    }

    /// Marks the session stopped: the hosted game ended.
    pub(crate) fn stop(&mut self) {
        if !self.ended {
            self.games_ended += 1;
        }
        self.ended = true;
    }

    /// A hosted game starts: its first step is a new session even on the
    /// server and thread of the one that stopped.
    pub(crate) fn begin(&mut self) {
        *self = Self {
            games_ended: self.games_ended,
            ..Self::default()
        };
    }

    /// The game the server steps, from 1, as
    /// [`TickData::game`](enshwick_core::abi::TickData::game) counts it.
    pub(crate) const fn game(&self) -> u64 {
        self.games_ended + 1
    }

    /// Follows a step as [`Session::step`] does; with the game it is in.
    pub(crate) fn step_in_game(&mut self, server: usize, thread: u32) -> (Step, u64) {
        (self.step(server, thread), self.game())
    }
}

/// The data of the every-step [`Event::TICKED`] for step `step` of `game`.
fn tick_data(step: u64, time: StepTime, game: u64) -> TickData {
    TickData {
        elapsed_nanos: time.elapsed,
        step_nanos: time.average,
        game,
        ..TickData::new(step)
    }
}

static SESSION: Mutex<Session> = Mutex::new(Session {
    server: 0,
    thread: 0,
    ended: false,
    games_ended: 0,
});

static LOADED: OnceLock<(&'static Loader, Offsets)> = OnceLock::new();
// SAFETY: `StepFn` is the signature of the build table's `server.post_step`,
// and `link` uses this slot only as that function's next.
static STEP_NEXT: Next<StepFn> = unsafe { Next::new() };
// SAFETY: `StateFn` is the signature of the build table's `server.set_state`,
// and `link` uses this slot only as that function's next.
static STATE_NEXT: Next<StateFn> = unsafe { Next::new() };
// SAFETY: `SaveFn` is the signature of the build table's `server.save_step`,
// and `link` uses this slot only as that function's next.
static SAVE_NEXT: Next<SaveFn> = unsafe { Next::new() };
// SAFETY: `RemoveFn` is the signature of the build table's
// `server.player_remove`, and `link` uses this slot only as that function's
// next.
static REMOVE_NEXT: Next<RemoveFn> = unsafe { Next::new() };
// SAFETY: `PlaceFn` is the signature of the build table's
// `building.spawn_prop`, and `link` uses this slot only as that function's
// next.
static PLACE_NEXT: Next<PlaceFn> = unsafe { Next::new() };
// SAFETY: `PickupFn` is the signature of the build table's `building.pickup`,
// and `link` uses this slot only as that function's next.
static PICKUP_NEXT: Next<PickupFn> = unsafe { Next::new() };
/// Building events from worker threads, raised after the step.
static BUILDING: Mutex<Vec<BuildingData>> = Mutex::new(Vec::new());
/// The handle of the player seen joining in each slot, 0 for none.
static JOINED: [AtomicU32; MAX_SLOTS] = [const { AtomicU32::new(0) }; MAX_SLOTS];
/// One fuse per link: a panic in the loader's own work of a link turns off
/// only that link's events, and the link forwards from then on.
static STEP_FUSE: Fuse = Fuse::new("step events");
static STATE_FUSE: Fuse = Fuse::new("world loaded and shutdown events");
static SAVE_FUSE: Fuse = Fuse::new("world saved events");
static REMOVE_FUSE: Fuse = Fuse::new("player left events");
static BUILDING_FUSE: Fuse = Fuse::new("building events");

/// The links' state once linked.
fn loaded() -> Option<&'static (&'static Loader, Offsets)> {
    LOADED.get()
}

/// Runs a link's own work under its fuse; a panic in it turns that link off
/// instead of unwinding into the game, which would end the process.
fn guarded(fuse: &Fuse, link: &str, work: impl FnOnce()) {
    let _ran = fuse.run(link, work);
}

/// Whether the loader's own work needs the link to `symbol` with no mod
/// subscribed to its events: the step tells the world reads the game's
/// thread, the state change loads each world's settings and record, and the
/// removal calls the parts that hear a player leave (`leaves`), such as item
/// data setting aside what the character carries.
fn own_link(symbol: Symbol, leaves: bool) -> bool {
    symbol == pinned::server::POST_STEP
        || symbol == pinned::server::SET_STATE
        || (leaves && symbol == pinned::server::PLAYER_REMOVE)
}

/// Runs one part of the post-step work under a fuse of its own, so a panic
/// in one feature's step leaves the others and the tick running.
macro_rules! part {
    ($feature:literal, $work:expr) => {{
        static FUSE: Fuse = Fuse::new($feature);
        let _ran = FUSE.run("the post-step link", || $work);
    }};
}

type StepFn = unsafe extern "system" fn(server: *mut u8);
type StateFn = unsafe extern "system" fn(entry: *mut u8, state: u8);
type SaveFn = unsafe extern "system" fn(out: *mut u8, save: *mut u8, context: *mut u8) -> *mut u8;
type RemoveFn = unsafe extern "system" fn(server: *mut u8, slot: *mut u8);
type PlaceFn = unsafe extern "system" fn(
    context: *mut u8,
    template: *const [u8; 16],
    position: *const [f32; 3],
    rotation: *const f32,
    item: u32,
    variant: u32,
    material: *const u8,
) -> u32;
type PickupFn = unsafe extern "system" fn(context: *mut u8, target: *const u8, tool: u32) -> u8;

/// Links what the subscriptions need; a missing symbol leaves its events
/// silent and says so.
pub(crate) fn link(loader: &'static Loader) {
    let wants = |events: &[Event]| events.iter().any(|&e| loader.subscribed(e));
    let constant = |name: Constant| {
        loader
            .build
            .constant(name.id())
            .and_then(|v| usize::try_from(v).ok())
    };
    let offsets = (|| {
        Some(Offsets {
            entry_state: constant(pinned::server::ENTRY_STATE)?,
            slots: constant(pinned::server::PLAYER_SLOTS)?,
            slot_size: constant(pinned::server::PLAYER_SLOT_SIZE)?,
            slot_count: constant(pinned::server::PLAYER_SLOT_COUNT)?.min(MAX_SLOTS),
            slot_state: constant(pinned::server::PLAYER_SLOT_STATE)?,
            slot_entity: constant(pinned::server::PLAYER_SLOT_ENTITY)?,
            slot_key: constant(pinned::server::PLAYER_SLOT_KEY)?,
            ctx_player: constant(pinned::building::CTX_PLAYER)?,
            ctx_replay: constant(pinned::building::CTX_REPLAY)?,
            pickup_entity: constant(pinned::building::PICKUP_ENTITY)?,
            pickup_mode: constant(pinned::building::PICKUP_MODE)?,
            server_world: constant(pinned::server::WORLD)?,
            entities: constant(pinned::world::ENTITIES)?,
        })
    })();
    let Some(offsets) = offsets else {
        crate::log!(WARN, "game events off: this build has no server layout");
        return;
    };
    if LOADED.set((loader, offsets)).is_err() {
        return;
    }
    let leaves = loader.parts.iter().any(|part| part.left.is_some());
    let link = |events: &[Event], symbol: Symbol| -> Option<Target<'static>> {
        (wants(events) || own_link(symbol, leaves)).then_some(Target::Pinned(symbol))
    };
    let none = HookFlags::NONE;
    // A feature per link: each serves its own events, and one missing
    // symbol must not take the step the whole loader runs on.
    let stepped = link(&[], pinned::server::POST_STEP).is_some_and(|target| {
        loader.link_feature(STEP_FUSE.feature(), target, &STEP_NEXT, step, none)
    });
    crate::engine::ticks::set_stepped(stepped);
    if let Some(target) = link(
        &[Event::WORLD_LOADED, Event::SHUTDOWN_STARTED],
        pinned::server::SET_STATE,
    ) {
        loader.link_feature(STATE_FUSE.feature(), target, &STATE_NEXT, set_state, none);
    }
    if let Some(target) = link(&[Event::WORLD_SAVED], pinned::server::SAVE_STEP) {
        loader.link_feature(SAVE_FUSE.feature(), target, &SAVE_NEXT, save, none);
    }
    if let Some(target) = link(
        &[Event::PLAYER_LEFT, Event::PLAYER_JOINED],
        pinned::server::PLAYER_REMOVE,
    ) {
        loader.link_feature(REMOVE_FUSE.feature(), target, &REMOVE_NEXT, remove, none);
    }
    // Building events wait in a queue only the step drains.
    if !stepped {
        crate::log!(WARN, "building events off: they need the server's step");
        return;
    }
    if let Some(target) = link(&[Event::BUILDING_PLACED], pinned::building::SPAWN_PROP) {
        loader.link_feature("building placed events", target, &PLACE_NEXT, place, none);
    }
    if let Some(target) = link(&[Event::BUILDING_REMOVED], pinned::building::PICKUP) {
        loader.link_feature(
            "building removed events",
            target,
            &PICKUP_NEXT,
            pickup,
            none,
        );
    }
}

/// The player in the server's player slot `slot`, number `slot_index`.
///
/// # Safety
///
/// - `slot` is one of the live server's player slots, readable for the
///   build table's `server.player_slot_size` bytes.
/// - Called on a thread the game reads the slot on, with no writer
///   meanwhile.
unsafe fn player(slot_index: usize, slot: *const u8, offsets: &Offsets) -> PlayerData {
    PlayerData {
        header: Header::of::<PlayerData>(),
        slot: u32::try_from(slot_index).unwrap_or(u32::MAX),
        // SAFETY: the caller guarantees a live player slot; its u32 entity
        // sits at the build table's `server.player_slot_entity` offset.
        entity: unsafe { read_at(slot, offsets.slot_entity) },
        // SAFETY: the caller guarantees a live player slot; its u64 account
        // key sits at the build table's `server.player_slot_key` offset.
        key: unsafe { read_at(slot, offsets.slot_key) },
    }
}

unsafe extern "system" fn step(server: *mut u8) {
    // SAFETY: this detour runs only as the `server.post_step` link, so the
    // slot holds the next function of that signature; the game's server
    // passes unchanged.
    unsafe { STEP_NEXT.get()(server) };
    let Some((loader, offsets)) = loaded().filter(|_| !STEP_FUSE.is_off()) else {
        return;
    };
    // The step's one clock read: every part that needs the time takes it.
    let now = Instant::now();
    // SAFETY: `server` is the post-step's own argument, the game's server or
    // null, and this runs on the game thread right after its step.
    guarded(&STEP_FUSE, "the post-step link", || unsafe {
        after_step(loader, offsets, server, now);
    });
    part!(
        "code patch tries",
        crate::engine::retry::after_step(loader, now)
    );
}

/// The post-step link's work on the game thread, after the game's step.
///
/// # Safety
///
/// - `server` is null or the game's live server, which holds
///   `server.player_slot_count` player slots of `server.player_slot_size`
///   bytes from the build table's `server.player_slots` offset.
/// - Called on the game thread after its step, where nothing else writes
///   the slots.
unsafe fn after_step(loader: &'static Loader, offsets: &Offsets, server: *mut u8, now: Instant) {
    // SAFETY: `GetCurrentThreadId` has no preconditions.
    let thread = unsafe { windows_sys::Win32::System::Threading::GetCurrentThreadId() };
    let (seen, game) = SESSION
        .relock()
        .step_in_game(server.expose_provenance(), thread);
    match seen {
        Step::Ended => return,
        Step::New if loader.side == Side::CLIENT => {
            forget_joined();
            crate::log!(
                INFO,
                "this client hosts a game on thread {thread}: server mods act"
            );
        }
        Step::New | Step::Same => {}
    }
    crate::engine::world::capture(
        server.expose_provenance(),
        thread,
        offsets.server_world,
        offsets.entities,
    );
    let built = std::mem::take(&mut *BUILDING.relock());
    if !BUILDING_FUSE.is_off() {
        guarded(&BUILDING_FUSE, "the post-step link", || {
            actions::console::remember(&built);
            let players = if built.is_empty() {
                Vec::new()
            } else {
                crate::engine::world::players(loader).unwrap_or_default()
            };
            for data in &built {
                let event = if data.kind == BuildingData::PLACED {
                    Event::BUILDING_PLACED
                } else {
                    Event::BUILDING_REMOVED
                };
                let emit = || loader.emit(event, None, (&raw const *data).cast());
                // A handler fills its texts in the language of the player who built.
                match players.iter().find(|p| p.entity == data.player) {
                    Some(player) => crate::engine::texts::for_player(player.key.get(), emit),
                    None => emit(),
                }
            }
        });
    }
    let stepping = Stepping {
        loader,
        now,
        step: crate::engine::ticks::count_step(),
    };
    loader.parts.run(Stage::Follow, &stepping);
    if crate::engine::loader::handles(Event::TICKED) || TICKS.listed() {
        let nanos = TICKS.nanos(now);
        let time = TICKS.advance(nanos);
        let tick = tick_data(stepping.step, time, game);
        part!(
            "tick events",
            loader.emit(Event::TICKED, None, (&raw const tick).cast::<c_void>())
        );
        part!(
            "periodic ticks",
            periodic_ticks(stepping.step, nanos, time, game)
        );
    }
    loader.parts.run(Stage::Input, &stepping);
    loader.parts.run(Stage::Output, &stepping);
    // SAFETY: `server` is null or the game's live server, and this runs on
    // the game thread after its step, both by this function's `# Safety`.
    unsafe { joins(loader, offsets, server) };
}

/// Calls `call` of every part that has one, each under its own fuse, as
/// `site`.
fn run_fused(
    loader: &'static Loader,
    site: &str,
    call: impl Fn(&'static Part) -> Option<&'static PlayerCall>,
    player: &PlayerData,
) {
    for fused in loader.parts.iter().filter_map(call) {
        let _ran = fused.fuse.run(site, || (fused.run)(loader, player));
    }
}

/// Emits a join for each player slot that reached the game since the last
/// step, and forgets the slots that emptied.
///
/// # Safety
///
/// As [`after_step`]: `server` is null or the game's live server, read on
/// the game thread after its step.
unsafe fn joins(loader: &'static Loader, offsets: &Offsets, server: *mut u8) {
    if server.is_null() {
        return;
    }
    for (index, joined) in JOINED.iter().enumerate().take(offsets.slot_count) {
        let slot = server
            .cast_const()
            .wrapping_add(offsets.slots + index * offsets.slot_size);
        // SAFETY: `server` is non-null by the check before it and live by the
        // caller's contract, and `index` is below its slot count, so `slot`
        // is one of its player slots, which starts with its u32 handle.
        let handle: u32 = unsafe { read_at(slot, 0) };
        // SAFETY: `slot` is a live player slot of the server; its u8 state
        // sits at the build table's `server.player_slot_state` offset.
        let state: u8 = unsafe { read_at(slot, offsets.slot_state) };
        // SAFETY: `slot` is a live player slot of the server; its u32 entity
        // sits at the build table's `server.player_slot_entity` offset.
        let entity: u32 = unsafe { read_at(slot, offsets.slot_entity) };
        if handle == 0 {
            joined.store(0, Ordering::Relaxed);
        } else if state == SLOT_IN_GAME && entity != 0 && joined.load(Ordering::Relaxed) != handle {
            joined.store(handle, Ordering::Relaxed);
            // SAFETY: `slot` is a live player slot of the server, read on the
            // game thread after its step, as the caller guarantees.
            let data = unsafe { player(index, slot, offsets) };
            run_fused(
                loader,
                "a player's join",
                |part| part.joined.as_ref(),
                &data,
            );
            part!(
                "player joined events",
                loader.emit(Event::PLAYER_JOINED, None, (&raw const data).cast())
            );
        }
    }
}

/// Calls the periodic tick handlers due after step `step` of `game` at
/// `now`, each with its mod's recent cost when callbacks are timed.
fn periodic_ticks(step: u64, now: u64, time: StepTime, game: u64) -> usize {
    TICKS.run(
        step,
        now,
        time,
        |tick| !crate::engine::loader::is_off(tick.owner()),
        |tick, mut data| {
            data.game = game;
            data.cost_nanos = crate::engine::profile::events_reading(tick.owner())
                .and_then(|(calls, ticks)| {
                    let (calls, ticks) = tick.since_seen(calls, ticks);
                    crate::engine::profile::nanos(ticks.checked_div(calls)?)
                })
                .unwrap_or(0);
            crate::engine::profile::call(
                tick.owner(),
                crate::engine::profile::Feature::Events,
                || {
                    // SAFETY: the handler lives in the mod's DLL, which stays
                    // loaded, with the pointer it was subscribed with; `data`
                    // lives for the call, and the safe layer catches the
                    // handler's panics.
                    unsafe {
                        (tick.callback())(
                            Event::TICKED,
                            (&raw const data).cast::<c_void>(),
                            tick.user(),
                        );
                    };
                },
            );
        },
    )
}

fn forget_joined() {
    for joined in &JOINED {
        joined.store(0, Ordering::Relaxed);
    }
}

/// Ends a hosted game on the client: its server goes away with it. The
/// world's settings unload first, while their mods' handlers still read the
/// server; each part forgets its server after it is gone.
fn end_session(loader: &'static Loader) {
    crate::base::worlds::unload(loader);
    SESSION.relock().stop();
    crate::engine::world::release();
    loader.parts.end(loader);
    forget_joined();
    BUILDING.relock().clear();
    crate::log!(
        INFO,
        "the hosted game stopped; server mods rest until the next"
    );
}

/// Drops every thread's read-only runs where their pages may go: the game
/// thread still reads after the shutdown (the final save) and frees the
/// world only in the teardown, and a new hosted game maps its own (server
/// 399, client 426).
fn forget_pages_entering(state: u8) {
    if matches!(state, STATE_TEAR_DOWN | STATE_INITIALIZE) {
        crate::engine::pages::forget();
    }
}

unsafe extern "system" fn set_state(entry: *mut u8, state: u8) {
    forget_pages_entering(state);
    if state == STATE_TEAR_DOWN
        && std::panic::catch_unwind(crate::engine::world::tear_down).is_err()
    {
        crate::log!(
            ERROR,
            "the server's teardown wait panicked; readers off the game thread may still hold it"
        );
    }
    let old = loaded()
        .filter(|_| !entry.is_null())
        // SAFETY: `entry` is non-null by the filter before it and is the hooked
        // function's server instance entry; its u8 state sits at the build
        // table's `server.entry_state` offset.
        .map(|(_, offsets)| unsafe { read_at::<u8>(entry, offsets.entry_state) });
    // SAFETY: this detour runs only as the `server.set_state` link, so the
    // slot holds the next function of that signature; the game's arguments
    // pass unchanged.
    unsafe { STATE_NEXT.get()(entry, state) };
    if state == STATE_SHUTDOWN {
        crate::engine::world::retire();
    }
    let (Some((loader, _)), Some(old)) = (loaded().filter(|_| !STATE_FUSE.is_off()), old) else {
        return;
    };
    guarded(&STATE_FUSE, "the server state link", || match state {
        STATE_INITIALIZE if old != STATE_INITIALIZE && loader.side == Side::CLIENT => {
            for hosting in loader.parts.iter().filter_map(|part| part.hosting) {
                hosting(loader);
            }
        }
        STATE_RUN if old != STATE_RUN => {
            if loader.side == Side::CLIENT {
                SESSION.relock().begin();
            }
            crate::base::worlds::load(loader);
            crate::start::record::write_for_world();
            loader.emit(Event::WORLD_LOADED, None, core::ptr::null());
        }
        STATE_SHUTDOWN if old != STATE_SHUTDOWN => {
            // The game asks for its final save on a later pass of the
            // server's loop, through the same request a hold refuses
            // (server 26430253).
            crate::base::saves::shut_down(loader, || {
                loader.emit(Event::SHUTDOWN_STARTED, None, core::ptr::null());
            });
            if loader.side == Side::CLIENT {
                end_session(loader);
            }
        }
        _ => {}
    });
}

unsafe extern "system" fn save(out: *mut u8, save: *mut u8, context: *mut u8) -> *mut u8 {
    let held = !world::transactions::holds().quiet();
    if held {
        guarded(&SAVE_FUSE, "the save link", || {
            world::transactions::saving(true);
        });
    }
    // SAFETY: this detour runs only as the `server.save_step` link, so the
    // slot holds the next function of that signature; the game's arguments
    // pass unchanged.
    let result = unsafe { SAVE_NEXT.get()(out, save, context) };
    if held {
        guarded(&SAVE_FUSE, "the save link", || {
            world::transactions::saving(false);
        });
    }
    // SAFETY: `out` is non-null by the check before it and is the save
    // step's result, whose first byte the game has just written.
    let done = !out.is_null() && unsafe { read_at::<u8>(out, 0) } == 0;
    if done && let Some((loader, _)) = loaded() {
        guarded(&SAVE_FUSE, "the save link", || {
            crate::base::saves::saved(loader);
            loader.emit(Event::WORLD_SAVED, None, core::ptr::null());
        });
    }
    result
}

unsafe extern "system" fn remove(server: *mut u8, slot: *mut u8) {
    if let Some((loader, offsets)) = loaded()
        && !server.is_null()
        && !slot.is_null()
    {
        let first = server.addr() + offsets.slots;
        let index = slot.addr().wrapping_sub(first) / offsets.slot_size.max(1);
        // SAFETY: `slot` is non-null by the check before it and is the hooked
        // removal's player slot argument, intact on entry; a slot starts with
        // its u32 handle.
        let handle: u32 = unsafe { read_at(slot, 0) };
        if handle != 0
            && let Some(joined) = JOINED.get(index).filter(|_| index < offsets.slot_count)
            && joined.load(Ordering::Relaxed) == handle
        {
            joined.store(0, Ordering::Relaxed);
            // SAFETY: `slot` is the removal's own player slot argument, below
            // the slot count by the check before it and intact on entry, on the
            // thread the game removes it on.
            let data = unsafe { player(index, slot, offsets) };
            // Before the game removes the character: its items still read.
            run_fused(
                loader,
                "the player removal link",
                |part| part.left.as_ref(),
                &data,
            );
            guarded(&REMOVE_FUSE, "the player removal link", || {
                loader.emit(Event::PLAYER_LEFT, None, (&raw const data).cast());
            });
        }
    }
    // SAFETY: this detour runs only as the `server.player_remove` link, so
    // the slot holds the next function of that signature; the game's
    // arguments pass unchanged.
    unsafe { REMOVE_NEXT.get()(server, slot) };
}

fn queue(data: BuildingData) {
    if BUILDING_FUSE.is_off() {
        return;
    }
    guarded(&BUILDING_FUSE, "a building link", || {
        BUILDING.relock().push(data);
    });
}

unsafe extern "system" fn place(
    context: *mut u8,
    template: *const [u8; 16],
    position: *const [f32; 3],
    rotation: *const f32,
    item: u32,
    variant: u32,
    material: *const u8,
) -> u32 {
    // SAFETY: this detour runs only as the `building.spawn_prop` link, so
    // the slot holds the next function of that signature; the game's
    // arguments pass unchanged.
    let entity = unsafe {
        PLACE_NEXT.get()(
            context, template, position, rotation, item, variant, material,
        )
    };
    if entity != 0
        && !context.is_null()
        && let Some((_, offsets)) = loaded()
    {
        queue(BuildingData {
            header: Header::of::<BuildingData>(),
            entity: enshwick_core::Entity(entity),
            // SAFETY: `context` is non-null by the check before it and is the
            // hooked function's build context; its u32 acting player sits at
            // the build table's `building.ctx_player` offset.
            player: unsafe { read_at(context, offsets.ctx_player) },
            item: enshwick_core::ItemId::new(item),
            kind: BuildingData::PLACED,
            // SAFETY: `context` is non-null by the check before it and is the
            // hooked function's build context; its u8 replay flag sits at the
            // build table's `building.ctx_replay` offset.
            replay: unsafe { read_at(context, offsets.ctx_replay) },
            template: if template.is_null() {
                [0; 16]
            } else {
                // SAFETY: `template` is non-null by the check before it and is the
                // hooked function's 16-byte template GUID argument.
                unsafe { read_at(template.cast(), 0) }
            },
            position: if position.is_null() {
                [0.0; 3]
            } else {
                // SAFETY: `position` is non-null by the check before it and is the
                // hooked function's three-float position argument.
                unsafe { read_at(position.cast(), 0) }
            },
        });
    }
    entity
}

unsafe extern "system" fn pickup(context: *mut u8, target: *const u8, tool: u32) -> u8 {
    // The target is the entity and the mode; read them while the entity lives.
    let before = loaded()
        .filter(|_| !context.is_null() && !target.is_null())
        .map(|(_, offsets)| {
            // SAFETY: `target` is non-null by the filter before it and is the
            // hooked function's target, whose entity still lives; its u32
            // entity sits at the build table's `building.pickup_entity`
            // offset.
            let entity: u32 = unsafe { read_at(target, offsets.pickup_entity) };
            // SAFETY: `target` is non-null by the filter before it and is the
            // hooked function's target; its u8 mode sits at the build table's
            // `building.pickup_mode` offset.
            let mode: u8 = unsafe { read_at(target, offsets.pickup_mode) };
            // SAFETY: `context` is non-null by the filter before it and is the
            // hooked function's build context; its u32 acting player sits at
            // the build table's `building.ctx_player` offset.
            let player: u32 = unsafe { read_at(context, offsets.ctx_player) };
            // SAFETY: `context` is non-null by the filter before it and is the
            // hooked function's build context; its u8 replay flag sits at the
            // build table's `building.ctx_replay` offset.
            let replay: u8 = unsafe { read_at(context, offsets.ctx_replay) };
            (entity, mode, player, replay)
        });
    // SAFETY: this detour runs only as the `building.pickup` link, so the
    // slot holds the next function of that signature; the game's arguments
    // pass unchanged.
    let done = unsafe { PICKUP_NEXT.get()(context, target, tool) };
    if done != 0
        && let Some((entity, mode, player, replay)) = before
    {
        queue(BuildingData {
            header: Header::of::<BuildingData>(),
            entity: enshwick_core::Entity(entity),
            player: enshwick_core::Entity(player),
            item: enshwick_core::ItemId::new(0),
            kind: if mode == 2 {
                BuildingData::TORN_DOWN
            } else {
                BuildingData::PICKED_UP
            },
            replay,
            template: [0; 16],
            position: [0.0; 3],
        });
    }
    done
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_panic_in_a_link_s_work_turns_off_that_link_alone() {
        let (one, other) = (Fuse::new("one"), Fuse::new("other"));
        let mut ran = false;
        guarded(&one, "test", || ran = true);
        assert!(ran && !one.is_off());
        guarded(&one, "test", || panic!("a loader bug"));
        assert!(one.is_off());
        guarded(&other, "test", || ran = false);
        assert!(!ran && !other.is_off());
    }

    #[test]
    fn a_panic_in_one_step_part_leaves_the_others_running() {
        let mut later = 0;
        for round in 0..3 {
            part!("test part that breaks", {
                assert!(round > 0, "a loader bug");
            });
            part!("test part after it", later += 1);
        }
        assert_eq!(later, 3);
    }

    #[test]
    fn read_only_runs_found_after_the_shutdown_are_forgotten_at_the_teardown_and_the_next_start() {
        use crate::engine::pages::LASTING;
        use windows_sys::Win32::System::Memory::{
            MEM_RELEASE, PAGE_READONLY, PAGE_READWRITE, VirtualFree,
        };
        let page = crate::engine::pages::allocate(None, LASTING).expect("fresh pages");
        let start = page.as_ptr().expose_provenance();
        for state in [STATE_TEAR_DOWN, STATE_INITIALIZE] {
            // SAFETY: the pages are the test's own allocation, used by no one else.
            unsafe { crate::engine::pages::protect(start, LASTING, PAGE_READONLY) }
                .expect("read-only");
            assert_eq!(
                crate::engine::pages::read_only(start, 16),
                16,
                "found in the final save"
            );
            // SAFETY: the pages are the test's own allocation, used by no one else.
            unsafe { crate::engine::pages::protect(start, LASTING, PAGE_READWRITE) }
                .expect("read-write");
            forget_pages_entering(state);
            assert_eq!(
                crate::engine::pages::read_only(start, 16),
                0,
                "state {state}"
            );
        }
        // SAFETY: `page` is the test's own allocation, released once.
        unsafe { VirtualFree(page.as_ptr().cast(), 0, MEM_RELEASE) };
    }

    #[test]
    fn a_world_loads_its_settings_on_a_server_whose_mods_subscribe_to_nothing() {
        assert!(own_link(pinned::server::SET_STATE, false));
        assert!(own_link(pinned::server::POST_STEP, false));
        assert!(!own_link(pinned::server::SAVE_STEP, true));
    }

    #[test]
    fn a_player_leaving_reaches_the_parts_that_hear_it_with_no_mod_subscribed() {
        assert!(own_link(pinned::server::PLAYER_REMOVE, true));
        assert!(!own_link(pinned::server::PLAYER_REMOVE, false));
        let parts = crate::engine::parts::Parts::new(crate::parts::LAYERS);
        assert!(parts.iter().any(|part| part.left.is_some()));
    }

    #[test]
    fn a_session_follows_its_server_and_thread() {
        let mut session = Session::default();
        assert_eq!(session.step(0x1000, 7), Step::New);
        assert_eq!(session.step(0x1000, 7), Step::Same);
        session.stop();
        assert_eq!(session.step(0x1000, 7), Step::Ended);
        session.begin();
        assert_eq!(session.step(0x1000, 7), Step::New);
        assert_eq!(session.step(0x1000, 7), Step::Same);
        // The next hosted game: a new server on a new thread.
        assert_eq!(session.step(0x2000, 9), Step::New);
        assert_eq!(session.step(0x2000, 9), Step::Same);
        // The same address again, on another thread, is another game.
        session.stop();
        assert_eq!(session.step(0x2000, 11), Step::New);
    }

    #[test]
    fn each_hosted_game_that_ends_makes_the_next_steps_another_game() {
        let mut session = Session::default();
        assert_eq!(session.step(0x1000, 7), Step::New);
        assert_eq!(session.game(), 1);
        session.begin();
        assert_eq!(session.step(0x1000, 7), Step::New);
        assert_eq!(session.game(), 1, "the game's own start is the same game");
        session.stop();
        session.stop();
        assert_eq!(session.game(), 2, "one game ended, once");
        session.begin();
        assert_eq!(session.step(0x2000, 9), Step::New);
        assert_eq!(session.game(), 2);
    }
}
