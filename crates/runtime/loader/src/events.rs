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
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Instant;

use enshwick_core::abi::{BuildingData, Event, Header, HookFlags, PlayerData, Side, TickData};
use enshwick_core::raw::read_at;

use crate::engine::fuse::Fuse;
use crate::engine::loader::{Loader, Target};
use crate::engine::parts::{Fused, Part, PlayerCall, Stage, Stepping, fused};
use crate::engine::pending::Pending;
use session::{SESSION, Session, Step};

mod session;
use crate::engine::pinned;
use crate::engine::pinned::{Constant, Symbol};
use crate::engine::ticks::{StepTime, TICKS};
use crate::{actions, world};

/// The step's own part: its build table ids and its links, which the
/// domains' links rely on.
pub(crate) static PART: Part = Part {
    table_ids: TABLE_IDS,
    link: Some(link),
    forget: Some(fused!("events forgetting a hosted game", |loader| forget(
        loader
    ))),
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
/// A server enters None as it leaves the game's server list, once the
/// teardown freed it (client 26430625). The name is Enshwick's.
const STATE_NONE: u8 = 0;
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

/// The data of the every-step [`Event::TICKED`] for step `step` of `game`.
fn tick_data(step: u64, time: StepTime, game: u64) -> TickData {
    TickData {
        elapsed_ns: time.elapsed,
        step_ns: time.average,
        game,
        ..TickData::new(step)
    }
}

/// The server layout the links read, once linked.
static OFFSETS: OnceLock<Offsets> = OnceLock::new();
/// The events domain's state of the running server's world, which a
/// client's hosted game's end forgets.
struct State {
    /// Building events from worker threads, raised after the step.
    building: Pending<BuildingData>,
    /// The handle of the player seen joining in each slot, 0 for none.
    joined: [AtomicU32; MAX_SLOTS],
}

static STATE: State = State {
    building: Pending::new(),
    joined: [const { AtomicU32::new(0) }; MAX_SLOTS],
};
/// One feature per group of events: a target this build lacks, or a panic in
/// the loader's work for those events, turns off that group alone.
static STEP_FUSE: Fuse = Fuse::new("step events");
static STATE_FUSE: Fuse = Fuse::new("world loaded and shutdown events");
static SAVE_FUSE: Fuse = Fuse::new("world saved events");
static REMOVE_FUSE: Fuse = Fuse::new("player left events");
static PLACE_FUSE: Fuse = Fuse::new("building placed events");
static PICKUP_FUSE: Fuse = Fuse::new("building removed events");
/// The links that also carry the engine's own work (the code patch tries,
/// the server's lifetime, the parts that hear a player leave), each a
/// feature apart from its events: a panic in an event's work turns off
/// those events, and the link stays wired for the rest. The save and the
/// building links carry only their events' work and link under the events'
/// fuse.
static STEP_LINK: Fuse = Fuse::new("the server's step link");
static STATE_LINK: Fuse = Fuse::new("the server's state link");
static REMOVE_LINK: Fuse = Fuse::new("the server's player removal link");

/// The post-step work beside the parts' steps, each under a fuse of its
/// own, so a panic in one leaves the others and the tick running.
static TRIES: Fused<fn(&'static Loader, Instant)> = fused!("code patch tries", |loader, now| {
    crate::engine::retry::after_step(loader, now);
});
/// The console's list of the pieces placed, apart from the building events.
static REMEMBERED: Fused<fn(&[BuildingData])> = fused!("the console's placed pieces", |built| {
    actions::console::remember(built);
});
static TICK_EVENTS: Fused<fn(&'static Loader, &TickData)> =
    fused!("tick events", |loader, tick| loader.emit(
        Event::TICKED,
        None,
        core::ptr::from_ref(tick).cast::<c_void>()
    ));
/// A run of the periodic tick handlers: the step and whether it enters a
/// game's world, the time, the step's time and the game.
type PeriodicRun = fn((u64, bool), u64, StepTime, u64);

static PERIODIC_TICKS: Fused<PeriodicRun> =
    fused!("periodic ticks", |step, nanos, time, game| periodic_ticks(
        step, nanos, time, game
    ));
static JOINED_EVENTS: Fused<fn(&'static Loader, &PlayerData)> = fused!(
    "player joined events",
    |loader, data| loader.emit(Event::PLAYER_JOINED, None, core::ptr::from_ref(data).cast())
);

/// The links' state once linked.
fn loaded() -> Option<(&'static Loader, &'static Offsets)> {
    crate::engine::loader::linked(&OFFSETS)
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

/// The server layout from the build table, or the first constant it lacks.
fn offsets(loader: &Loader) -> Result<Offsets, Constant> {
    let constant = |name: Constant| {
        loader
            .build
            .constant(name.id())
            .and_then(|v| usize::try_from(v).ok())
            .ok_or(name)
    };
    Ok(Offsets {
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
}

/// Links what the subscriptions and the loader's own work need, each link
/// a feature of its own; a link this build cannot make turns its feature
/// off and says so.
pub(crate) fn link(loader: &'static Loader) {
    let leaves = loader.parts.iter().any(|part| part.left.is_some());
    let wanted = |events: &[Event], symbol: Symbol| {
        own_link(symbol, leaves) || events.iter().any(|&e| loader.subscribed(e))
    };
    let step = wanted(&[], pinned::server::POST_STEP);
    let state = wanted(
        &[Event::WORLD_LOADED, Event::SHUTDOWN_STARTED],
        pinned::server::SET_STATE,
    );
    let save = wanted(&[Event::WORLD_SAVED], pinned::server::SAVE_STEP);
    let remove = wanted(
        &[Event::PLAYER_LEFT, Event::PLAYER_JOINED],
        pinned::server::PLAYER_REMOVE,
    );
    let place = wanted(&[Event::BUILDING_PLACED], pinned::building::SPAWN_PROP);
    let pickup = wanted(&[Event::BUILDING_REMOVED], pinned::building::PICKUP);
    let features = [
        (step, &STEP_FUSE),
        (state, &STATE_FUSE),
        (save, &SAVE_FUSE),
        (remove, &REMOVE_FUSE),
        (place, &PLACE_FUSE),
        (pickup, &PICKUP_FUSE),
    ];
    let offsets = match offsets(loader) {
        Ok(offsets) => offsets,
        Err(lacking) => {
            for (_, fuse) in features.iter().filter(|(wanted, _)| *wanted) {
                let _off = loader.fuse_off(fuse, lacking.id(), crate::engine::builds::ABSENT);
            }
            untold(loader, lacking.id(), crate::engine::builds::ABSENT);
            return;
        }
    };
    if OFFSETS.set(offsets).is_err() {
        return;
    }
    let stepped = link_server(loader, [step, state, save, remove]);
    crate::engine::ticks::set_stepped(stepped);
    link_building(loader, stepped, place, pickup);
}

/// Links the server's step, state change, save and player removal, each
/// one `wanted`; whether the step linked. Each link that fails has turned
/// its own feature off and said so.
fn link_server(loader: &'static Loader, wanted: [bool; 4]) -> bool {
    let [step, state, save, remove] = wanted;
    let at = Target::Pinned;
    let none = HookFlags::NONE;
    let stepped = step
        && loader.link_fused(
            &STEP_LINK,
            at(pinned::server::POST_STEP),
            &STEP_NEXT,
            step_link,
            none,
        );
    let state_linked = state
        && loader.link_fused(
            &STATE_LINK,
            at(pinned::server::SET_STATE),
            &STATE_NEXT,
            set_state,
            none,
        );
    if !state_linked {
        untold(loader, pinned::server::SET_STATE.id(), UNTOLD);
    }
    let _save = save
        && loader.link_fused(
            &SAVE_FUSE,
            at(pinned::server::SAVE_STEP),
            &SAVE_NEXT,
            save_link,
            none,
        );
    let removes = remove
        && loader.link_fused(
            &REMOVE_LINK,
            at(pinned::server::PLAYER_REMOVE),
            &REMOVE_NEXT,
            remove_link,
            none,
        );
    crate::engine::world::permissions::hear_leaves(removes);
    stepped
}

/// Why a client's own game events and notices are off without the state
/// link.
const UNTOLD: &str =
    "is not linked, and a hosted game's world cannot be told from the client's own";

/// On a client, turns its own game events and notices off by the entry `id`
/// for `reason`: without the state link no hosted server's start is seen,
/// so its world would pass for the client's own.
fn untold(loader: &Loader, id: &str, reason: &str) {
    if loader.side == Side::CLIENT {
        crate::base::game_events::untold(loader, id, reason);
    }
}

/// Links the building events a mod subscribed to, `place` and `pickup`,
/// which wait in a queue only the step drains: without the step's link
/// (`stepped`) they are off.
fn link_building(loader: &'static Loader, stepped: bool, place: bool, pickup: bool) {
    if !stepped {
        let links = [(place, &PLACE_FUSE), (pickup, &PICKUP_FUSE)];
        for (_, fuse) in links.iter().filter(|(wanted, _)| *wanted) {
            let _off = loader.fuse_off(
                fuse,
                pinned::server::POST_STEP.id(),
                "is not linked, and building events wait for the server's step",
            );
        }
        return;
    }
    let none = HookFlags::NONE;
    let _place = place
        && loader.link_fused(
            &PLACE_FUSE,
            Target::Pinned(pinned::building::SPAWN_PROP),
            &PLACE_NEXT,
            place_link,
            none,
        );
    let _pickup = pickup
        && loader.link_fused(
            &PICKUP_FUSE,
            Target::Pinned(pinned::building::PICKUP),
            &PICKUP_NEXT,
            pickup_link,
            none,
        );
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

crate::engine::chains::chain_detour! {
    /// The table's `server.post_step`, after which the loader's step runs.
    fn step_link(server: *mut u8) -> ();
    next STEP_NEXT;
    |onward| {
        onward.call();
        let Some((loader, offsets)) = loaded() else {
            return;
        };
        // The step's one clock read: every part that needs the time takes it.
        let now = Instant::now();
        onward.run(&STEP_FUSE, "the post-step link", |_| {
            // SAFETY: `server` is the post-step's own argument, the game's
            // server or null, and this runs on the game thread right after
            // its step.
            unsafe { after_step(loader, offsets, server, now) };
        });
        let _tried = TRIES.call("the post-step link", |tries| tries(loader, now));
    }
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
    let thread = crate::engine::off_thread::current_thread();
    let (seen, game) = SESSION.step(server.expose_provenance(), thread);
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
    let built = STATE.building.take();
    if !built.is_empty() {
        raise_building(loader, &built);
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
        let site = "the post-step link";
        let _raised = TICK_EVENTS.call(site, |raise| raise(loader, &tick));
        let _ran = PERIODIC_TICKS.call(site, |run| run((stepping.step, false), nanos, time, game));
    }
    loader.parts.run(Stage::Input, &stepping);
    loader.parts.run(Stage::Output, &stepping);
    // SAFETY: `server` is null or the game's live server, and this runs on
    // the game thread after its step, both by this function's `# Safety`.
    unsafe { joins(loader, offsets, server) };
}

/// The fuse of the link that queued a building event of `kind`.
fn building_fuse(kind: u8) -> &'static Fuse {
    if kind == BuildingData::PLACED {
        &PLACE_FUSE
    } else {
        &PICKUP_FUSE
    }
}

/// Raises the building events `built` the links queued since the last
/// step, each under the fuse of the link that queued it, which also covers
/// the players read for their language.
fn raise_building(loader: &'static Loader, built: &[BuildingData]) {
    let site = "the post-step link";
    let _kept = REMEMBERED.call(site, |remember| remember(built));
    let players = core::cell::OnceCell::new();
    Fuse::run_each(
        built,
        site,
        |data| building_fuse(data.kind),
        |data| {
            let event = if data.kind == BuildingData::PLACED {
                Event::BUILDING_PLACED
            } else {
                Event::BUILDING_REMOVED
            };
            let emit = || loader.emit(event, None, core::ptr::from_ref(data).cast());
            // A handler fills its texts in the language of the player who built.
            let builder = crate::engine::loader::handles(event)
                .then(|| {
                    players
                        .get_or_init(|| crate::engine::world::players(loader).unwrap_or_default())
                })
                .and_then(|players: &Vec<_>| players.iter().find(|p| p.entity == data.character));
            match builder {
                Some(player) => crate::engine::texts::for_reader(
                    Some(
                        crate::base::net::language_of(player.key.get())
                            .unwrap_or(enshwick_core::i18n::Language::FALLBACK),
                    ),
                    emit,
                ),
                None => emit(),
            }
        },
    );
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
        let _ran = fused.call(site, |run| run(loader, player));
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
    for (index, joined) in STATE.joined.iter().enumerate().take(offsets.slot_count) {
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
            let _raised = JOINED_EVENTS.call("a player's join", |raise| raise(loader, &data));
        }
    }
}

/// Calls the periodic tick handlers due after step `step` of `game` at
/// `now`, or every one not paused when `entering` its world, each with its
/// mod's recent cost when callbacks are timed.
fn periodic_ticks((step, entering): (u64, bool), now: u64, time: StepTime, game: u64) -> usize {
    TICKS.run(
        (step, entering),
        now,
        time,
        |tick| !crate::engine::loader::is_off(tick.owner()),
        |tick, mut data| {
            data.game = game;
            data.cost_ns = crate::engine::profile::events_reading(tick.owner())
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

/// A game's world starts: each periodic tick handler not paused runs once in
/// that game before the world's own handlers, so a scheduler learns the
/// game before a world load or join handler queues a task in it.
fn enter_ticks() {
    if !TICKS.listed() {
        return;
    }
    let nanos = TICKS.nanos(Instant::now());
    let (step, game) = (crate::engine::ticks::steps(), SESSION.game());
    let _ran = PERIODIC_TICKS.call("the world's start", |run| {
        run((step, true), nanos, StepTime::default(), game);
    });
}

/// Forgets the players seen joining and the building events queued in a
/// client's hosted game that ended.
fn forget(_loader: &'static Loader) {
    forget_joined();
    STATE.building.clear();
}

fn forget_joined() {
    for joined in &STATE.joined {
        joined.store(0, Ordering::Relaxed);
    }
}

/// Ends a hosted game on the client: its server goes away with it. The
/// world's settings unload first, while their mods' handlers still read the
/// server; each part forgets its server after it is gone.
fn end_session(loader: &'static Loader) {
    crate::base::worlds::unload(loader);
    SESSION.change(Session::stop);
    crate::engine::world::release();
    loader.parts.end(loader);
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

/// The engine's own work at `site` once the game changed the server to
/// `state`: a shutting server retires, and one that left the game's server
/// list frees its hosted thread.
fn after_change(site: &str, state: u8) {
    if state == STATE_SHUTDOWN {
        each_change(site, crate::engine::world::retire);
    }
    if state == STATE_NONE {
        each_change(site, crate::engine::world::server_left);
    }
}

/// Runs the engine's own `work` for the server's lifetime at `site`, on
/// every state change, even after it panicked once: turned off, it would
/// leave readers off the game thread a server the game frees, which crashes
/// the game, so a panic is logged and the next change runs it again.
fn each_change(site: &str, work: impl FnOnce()) {
    if let Err(payload) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(work)) {
        crate::log!(
            ERROR,
            "the loader panicked in {site}: {}; the next state change runs it again",
            crate::engine::early::panic_text(payload.as_ref())
        );
    }
}

crate::engine::chains::chain_detour! {
    /// The table's `server.set_state`: the world loads its settings and
    /// record when the server runs, and the shutdown and a hosted game's
    /// end reach the parts. The engine's own work around the change (the
    /// teardown wait, the pages forgotten, the server retired) runs on
    /// every call the link sees, whatever became of the events.
    fn set_state(entry: *mut u8, state: u8) -> ();
    next STATE_NEXT;
    |onward| {
        let site = "the server state link";
        each_change(site, || forget_pages_entering(state));
        if state == STATE_TEAR_DOWN {
            each_change(site, crate::engine::world::tear_down);
        }
        let linked = loaded().filter(|_| !entry.is_null());
        // SAFETY: `entry` is non-null by the filter before it and is the
        // hooked function's server instance entry; its u8 state sits at the
        // build table's `server.entry_state` offset.
        let old = linked.map(|(_, offsets)| unsafe { read_at::<u8>(entry, offsets.entry_state) });
        // Before the game's own change: loading the new server's world may
        // already move its events.
        if state == STATE_INITIALIZE
            && old.is_some_and(|old| old != STATE_INITIALIZE)
            && linked.is_some_and(|(loader, _)| loader.side == Side::CLIENT)
        {
            each_change(site, crate::engine::world::server_starts);
        }
        onward.call();
        after_change(site, state);
        let (Some((loader, _)), Some(old)) = (linked, old) else {
            return;
        };
        onward.run(&STATE_FUSE, site, |_| state_changed(loader, old, state));
    }
}

/// The server entered `state` from `old`: what the world's events and the
/// parts do then.
fn state_changed(loader: &'static Loader, old: u8, state: u8) {
    match state {
        STATE_INITIALIZE if old != STATE_INITIALIZE && loader.side == Side::CLIENT => {
            loader.parts.hosting(loader);
        }
        STATE_RUN if old != STATE_RUN => {
            if loader.side == Side::CLIENT {
                SESSION.change(Session::begin);
            }
            crate::base::worlds::load(loader);
            crate::start::record::write_for_world();
            enter_ticks();
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
    }
}

crate::engine::chains::chain_detour! {
    /// The table's `server.save_step`: world writes hold still while the
    /// game saves, and a finished save raises the world saved event.
    fn save_link(out: *mut u8, save: *mut u8, context: *mut u8) -> *mut u8;
    next SAVE_NEXT;
    |onward| {
        let held = !world::transactions::holds().quiet();
        if held {
            onward.before(&SAVE_FUSE, "the save link", || world::transactions::saving(true));
        }
        onward.run(&SAVE_FUSE, "the save link", |onward| {
            let result = onward.call();
            if held {
                world::transactions::saving(false);
            }
            // SAFETY: `out` is non-null by the check before it and is the
            // save step's result, whose first byte the game has just written.
            let done = !out.is_null() && unsafe { read_at::<u8>(out, 0) } == 0;
            if done && let Some((loader, _)) = loaded() {
                crate::base::saves::saved(loader);
                loader.emit(Event::WORLD_SAVED, None, core::ptr::null());
            }
            result
        })
    }
}

crate::engine::chains::chain_detour! {
    /// The table's `server.player_remove`: the parts that hear a player
    /// leave and the player left event, before the game removes the
    /// character, while its items still read.
    fn remove_link(server: *mut u8, slot: *mut u8) -> ();
    next REMOVE_NEXT;
    |onward| {
        if let Some((loader, offsets)) = loaded()
            && !server.is_null()
            && !slot.is_null()
        {
            onward.before(&REMOVE_LINK, "the player removal link", || {
                // SAFETY: `server` and `slot` are non-null by the checks
                // before them and are the removal's own arguments, intact on
                // entry, on the thread the game removes the player on.
                unsafe { player_leaves(loader, offsets, server, slot) };
            });
        }
        onward.call();
    }
}

/// Tells the parts and the mods that the player in `slot` of `server`
/// leaves, when it was seen joining: the parts each under its own fuse, the
/// player left event under the events' fuse.
///
/// # Safety
///
/// - `server` is the game's live server and `slot` one of its player slots,
///   readable for the build table's `server.player_slot_size` bytes.
/// - Called on the thread the game removes the player on, before it does.
unsafe fn player_leaves(
    loader: &'static Loader,
    offsets: &Offsets,
    server: *mut u8,
    slot: *mut u8,
) {
    let first = server.addr() + offsets.slots;
    let index = slot.addr().wrapping_sub(first) / offsets.slot_size.max(1);
    // SAFETY: `slot` is a live player slot by the caller's contract; a slot
    // starts with its u32 handle.
    let handle: u32 = unsafe { read_at(slot, 0) };
    let Some(joined) = STATE.joined.get(index).filter(|joined| {
        handle != 0 && index < offsets.slot_count && joined.load(Ordering::Relaxed) == handle
    }) else {
        return;
    };
    joined.store(0, Ordering::Relaxed);
    // SAFETY: `slot` is a live player slot below the slot count, read on the
    // thread the game removes it on, by the caller's contract and the
    // filter before it.
    let data = unsafe { player(index, slot, offsets) };
    let site = "the player removal link";
    run_fused(loader, site, |part| part.left.as_ref(), &data);
    let _raised = REMOVE_FUSE.run(site, || {
        loader.emit(Event::PLAYER_LEFT, None, core::ptr::from_ref(&data).cast());
    });
}

crate::engine::chains::chain_detour! {
    /// The table's `building.spawn_prop`: a placed building piece, queued
    /// for the step to raise, since building runs inside ECS systems on
    /// worker threads.
    fn place_link(
        context: *mut u8,
        template: *const [u8; 16],
        position: *const [f32; 3],
        rotation: *const f32,
        item: u32,
        variant: u32,
        material: *const u8,
    ) -> u32;
    next PLACE_NEXT;
    fuse &PLACE_FUSE, "the building placed link";
    |onward| {
        let entity = onward.call();
        if entity != 0
            && !context.is_null()
            && let Some((_, offsets)) = loaded()
        {
            STATE.building.push(BuildingData {
                header: Header::of::<BuildingData>(),
                entity: enshwick_core::Entity(entity),
                // SAFETY: `context` is non-null by the check before it and is
                // the hooked function's build context; its u32 acting player
                // sits at the build table's `building.ctx_player` offset.
                character: unsafe { read_at(context, offsets.ctx_player) },
                item: enshwick_core::ItemId::new(item),
                kind: BuildingData::PLACED,
                // SAFETY: `context` is non-null by the check before it and is
                // the hooked function's build context; its u8 replay flag sits
                // at the build table's `building.ctx_replay` offset.
                replay: unsafe { read_at(context, offsets.ctx_replay) },
                template: if template.is_null() {
                    enshwick_core::content::Guid::NIL
                } else {
                    // SAFETY: `template` is non-null by the check before it and
                    // is the hooked function's 16-byte template GUID argument.
                    unsafe { read_at(template.cast(), 0) }
                },
                // SAFETY: `position` is null or the hooked function's
                // three-float position argument, live for the call.
                position: unsafe { placed_at(position) },
            });
        }
        entity
    }
}

/// The game's three-float placing position in metres, the origin when null.
///
/// # Safety
///
/// `position` is null or valid for reads of 12 bytes for the call.
unsafe fn placed_at(position: *const [f32; 3]) -> [f64; 3] {
    if position.is_null() {
        return [0.0; 3];
    }
    // SAFETY: `position` is non-null by the check above and valid for 12
    // bytes by the caller's guarantee; the game passes three f32s, never f64s.
    unsafe { read_at::<[f32; 3]>(position.cast(), 0) }.map(f64::from)
}

crate::engine::chains::chain_detour! {
    /// The table's `building.pickup`:a building piece picked up or torn
    /// down, its entity and mode read while it lives, queued for the step
    /// to raise.
    fn pickup_link(context: *mut u8, target: *const u8, tool: u32) -> u8;
    next PICKUP_NEXT;
    fuse &PICKUP_FUSE, "the building removed link";
    |onward| {
        let before = loaded()
            .filter(|_| !context.is_null() && !target.is_null())
            .map(|(_, offsets)| {
                // SAFETY: `target` is non-null by the filter before it and is
                // the hooked function's target, whose entity still lives; its
                // u32 entity sits at the build table's `building.pickup_entity`
                // offset.
                let entity: u32 = unsafe { read_at(target, offsets.pickup_entity) };
                // SAFETY: `target` is non-null by the filter before it and is
                // the hooked function's target; its u8 mode sits at the build
                // table's `building.pickup_mode` offset.
                let mode: u8 = unsafe { read_at(target, offsets.pickup_mode) };
                // SAFETY: `context` is non-null by the filter before it and is
                // the hooked function's build context; its u32 acting player
                // sits at the build table's `building.ctx_player` offset.
                let player: u32 = unsafe { read_at(context, offsets.ctx_player) };
                // SAFETY: `context` is non-null by the filter before it and is
                // the hooked function's build context; its u8 replay flag sits
                // at the build table's `building.ctx_replay` offset.
                let replay: u8 = unsafe { read_at(context, offsets.ctx_replay) };
                (entity, mode, player, replay)
            });
        let done = onward.call();
        if done != 0
            && let Some((entity, mode, player, replay)) = before
        {
            STATE.building.push(BuildingData {
                header: Header::of::<BuildingData>(),
                entity: enshwick_core::Entity(entity),
                character: enshwick_core::Entity(player),
                item: enshwick_core::ItemId::new(0),
                kind: if mode == 2 {
                    BuildingData::TORN_DOWN
                } else {
                    BuildingData::PICKED_UP
                },
                replay,
                template: enshwick_core::content::Guid::NIL,
                position: [0.0; 3],
            });
        }
        done
    }
}

#[cfg(test)]
mod tests;
