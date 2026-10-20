//! Game events on the server, raised from the loader's own links: tick, world
//! loaded and saved, player joined and left, shutdown, building placed and
//! removed. The post-step link is always made, since it also tells the world
//! reads which thread is the game's; the others only when a mod subscribed to
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
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock, PoisonError};

use enshwick_loader_api::abi::{
    BuildingData, Event, Header, HookFlags, PlayerData, Side, TickData,
};

use crate::chains::{LOADER, Link};
use crate::host::Loader;

/// Server states, from the game's state machine.
const STATE_RUN: u8 = 5;
const STATE_SHUTDOWN: u8 = 6;
/// The slot state of a player in the world.
const SLOT_IN_GAME: u8 = 3;
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
        };
        Step::New
    }

    /// Marks the session stopped.
    pub(crate) fn stop(&mut self) {
        self.ended = true;
    }
}

static SESSION: Mutex<Session> = Mutex::new(Session {
    server: 0,
    thread: 0,
    ended: false,
});

static LOADED: OnceLock<(&'static Loader, Offsets)> = OnceLock::new();
static STEP_NEXT: AtomicUsize = AtomicUsize::new(0);
static STATE_NEXT: AtomicUsize = AtomicUsize::new(0);
static SAVE_NEXT: AtomicUsize = AtomicUsize::new(0);
static REMOVE_NEXT: AtomicUsize = AtomicUsize::new(0);
static PLACE_NEXT: AtomicUsize = AtomicUsize::new(0);
static PICKUP_NEXT: AtomicUsize = AtomicUsize::new(0);
/// Building events from worker threads, raised after the step.
static BUILDING: Mutex<Vec<BuildingData>> = Mutex::new(Vec::new());
static STEPS: AtomicU64 = AtomicU64::new(0);
/// The handle of the player seen joining in each slot, 0 for none.
static JOINED: [AtomicU32; MAX_SLOTS] = [const { AtomicU32::new(0) }; MAX_SLOTS];
/// Set once the loader's own work in a link panicked: from then on the links
/// only forward, and game events stay silent.
static OFF: AtomicBool = AtomicBool::new(false);

/// The links' state, unless the events are off.
fn loaded() -> Option<&'static (&'static Loader, Offsets)> {
    if OFF.load(Ordering::Relaxed) {
        return None;
    }
    LOADED.get()
}

/// Runs a link's own work; a panic in it turns the game events off instead
/// of unwinding into the game, which would end the process.
fn guarded(link: &str, work: impl FnOnce()) {
    if std::panic::catch_unwind(std::panic::AssertUnwindSafe(work)).is_err()
        && !OFF.swap(true, Ordering::Relaxed)
    {
        crate::log!(ERROR, "game events off: the {link} link panicked");
    }
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
    let constant = |name: &str| {
        loader
            .build
            .constant(name)
            .and_then(|v| usize::try_from(v).ok())
    };
    let offsets = (|| {
        Some(Offsets {
            entry_state: constant("server.entry_state")?,
            slots: constant("server.player_slots")?,
            slot_size: constant("server.player_slot_size")?,
            slot_count: constant("server.player_slot_count")?.min(MAX_SLOTS),
            slot_state: constant("server.player_slot_state")?,
            slot_entity: constant("server.player_slot_entity")?,
            slot_key: constant("server.player_slot_key")?,
            ctx_player: constant("building.ctx_player")?,
            ctx_replay: constant("building.ctx_replay")?,
            server_world: constant("server.world")?,
            entities: constant("world.entities")?,
        })
    })();
    let Some(offsets) = offsets else {
        crate::log!(WARN, "game events off: this build has no server layout");
        return;
    };
    if LOADED.set((loader, offsets)).is_err() {
        return;
    }
    let links: [(&[Event], &str, *const (), &'static AtomicUsize); 6] = [
        (&[], "server.post_step", step as *const (), &STEP_NEXT),
        (
            &[Event::WORLD_LOADED, Event::SHUTDOWN],
            "server.set_state",
            set_state as *const (),
            &STATE_NEXT,
        ),
        (
            &[Event::WORLD_SAVED],
            "server.save_step",
            save as *const (),
            &SAVE_NEXT,
        ),
        (
            &[Event::PLAYER_LEFT, Event::PLAYER_JOINED],
            "server.player_remove",
            remove as *const (),
            &REMOVE_NEXT,
        ),
        (
            &[Event::BUILDING_PLACED],
            "building.spawn_prop",
            place as *const (),
            &PLACE_NEXT,
        ),
        (
            &[Event::BUILDING_REMOVED],
            "building.pickup",
            pickup as *const (),
            &PICKUP_NEXT,
        ),
    ];
    // A client follows every hosted game's start and stop.
    let client = loader.side == Side::CLIENT;
    for (events, symbol, detour, next) in links {
        let session = client && symbol == "server.set_state";
        if !events.is_empty() && !wants(events) && !session {
            continue;
        }
        let Some(rva) = loader.build.symbol(symbol) else {
            crate::log!(WARN, "game events through {symbol} off: not in this build");
            continue;
        };
        let link = Link {
            owner: LOADER,
            detour: detour.addr(),
            next,
            priority: i32::MIN,
            flags: HookFlags::NONE,
        };
        if let Ok(mut chains) = loader.chains.lock() {
            let status = chains.add(loader.base + rva, link);
            if !status.is_ok() {
                crate::log!(
                    WARN,
                    "game events through {symbol} off: refused ({})",
                    status.0
                );
            }
        }
    }
}

/// Reads a `T` at `base + offset` in the game's memory.
///
/// # Safety
///
/// The address holds a readable `T`.
unsafe fn read<T: Copy>(base: *const u8, offset: usize) -> T {
    // SAFETY: forwarded from the caller.
    unsafe { base.wrapping_add(offset).cast::<T>().read_unaligned() }
}

fn next<F: Copy>(slot: &AtomicUsize) -> F {
    let address = slot.load(Ordering::Acquire);
    // SAFETY: the loader points the slot at the rest of the chain, which has
    // the hooked function's signature, before the link can run.
    unsafe { core::mem::transmute_copy::<usize, F>(&address) }
}

fn player(slot_index: usize, slot: *const u8, offsets: &Offsets) -> PlayerData {
    PlayerData {
        header: Header::of::<PlayerData>(),
        slot: u32::try_from(slot_index).unwrap_or(u32::MAX),
        // SAFETY: the slot is one of the server's player slots.
        entity: unsafe { read(slot, offsets.slot_entity) },
        // SAFETY: as above.
        key: unsafe { read(slot, offsets.slot_key) },
    }
}

unsafe extern "system" fn step(server: *mut u8) {
    // SAFETY: forwarded unchanged.
    unsafe { next::<StepFn>(&STEP_NEXT)(server) };
    let Some((loader, offsets)) = loaded() else {
        return;
    };
    // SAFETY: the game's server, as passed to the step.
    guarded("post-step", || unsafe {
        after_step(loader, offsets, server);
    });
}

/// The post-step link's work on the game thread, after the game's step.
///
/// # Safety
///
/// `server` is the game's server, or null.
unsafe fn after_step(loader: &'static Loader, offsets: &Offsets, server: *mut u8) {
    // SAFETY: no preconditions.
    let thread = unsafe { windows_sys::Win32::System::Threading::GetCurrentThreadId() };
    let seen = SESSION
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .step(server.addr(), thread);
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
    crate::world::capture(
        server.addr(),
        thread,
        offsets.server_world,
        offsets.entities,
    );
    let built = std::mem::take(&mut *BUILDING.lock().unwrap_or_else(PoisonError::into_inner));
    for data in &built {
        let event = if data.kind == BuildingData::PLACED {
            Event::BUILDING_PLACED
        } else {
            Event::BUILDING_REMOVED
        };
        loader.emit(event, None, (&raw const *data).cast());
    }
    crate::inventory::deliver();
    crate::altars::after_step(loader);
    let step = STEPS.fetch_add(1, Ordering::Relaxed) + 1;
    crate::interactions::after_step(loader, step);
    let tick = TickData {
        header: Header::of::<TickData>(),
        step,
    };
    loader.emit(Event::TICK, None, (&raw const tick).cast::<c_void>());
    crate::chat::drain();
    if server.is_null() {
        return;
    }
    for (index, joined) in JOINED.iter().enumerate().take(offsets.slot_count) {
        let slot = server
            .cast_const()
            .wrapping_add(offsets.slots + index * offsets.slot_size);
        // SAFETY: the server holds `slot_count` slots from `slots`.
        let handle: u32 = unsafe { read(slot, 0) };
        // SAFETY: as above.
        let state: u8 = unsafe { read(slot, offsets.slot_state) };
        // SAFETY: as above.
        let entity: u32 = unsafe { read(slot, offsets.slot_entity) };
        if handle == 0 {
            joined.store(0, Ordering::Relaxed);
        } else if state == SLOT_IN_GAME && entity != 0 && joined.load(Ordering::Relaxed) != handle {
            joined.store(handle, Ordering::Relaxed);
            let data = player(index, slot, offsets);
            loader.emit(Event::PLAYER_JOINED, None, (&raw const data).cast());
        }
    }
}

fn forget_joined() {
    for joined in &JOINED {
        joined.store(0, Ordering::Relaxed);
    }
}

/// Ends a hosted game on the client: its server goes away with it.
fn end_session() {
    SESSION
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .stop();
    crate::world::release();
    crate::altars::forget();
    crate::survivors::forget();
    crate::interactions::forget();
    forget_joined();
    BUILDING
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clear();
    crate::log!(
        INFO,
        "the hosted game stopped; server mods rest until the next"
    );
}

unsafe extern "system" fn set_state(entry: *mut u8, state: u8) {
    let old = loaded()
        .filter(|_| !entry.is_null())
        // SAFETY: the entry is the game's server instance entry.
        .map(|(_, offsets)| unsafe { read::<u8>(entry, offsets.entry_state) });
    // SAFETY: forwarded unchanged.
    unsafe { next::<StateFn>(&STATE_NEXT)(entry, state) };
    let (Some((loader, _)), Some(old)) = (loaded(), old) else {
        return;
    };
    guarded("server state", || match state {
        STATE_RUN if old != STATE_RUN => loader.emit(Event::WORLD_LOADED, None, core::ptr::null()),
        STATE_SHUTDOWN if old != STATE_SHUTDOWN => {
            loader.emit(Event::SHUTDOWN, None, core::ptr::null());
            if loader.side == Side::CLIENT {
                end_session();
                crate::chat::forget();
            }
        }
        _ => {}
    });
}

unsafe extern "system" fn save(out: *mut u8, save: *mut u8, context: *mut u8) -> *mut u8 {
    // SAFETY: forwarded unchanged.
    let result = unsafe { next::<SaveFn>(&SAVE_NEXT)(out, save, context) };
    // SAFETY: the game wrote its result's first byte through `out`.
    let done = !out.is_null() && unsafe { out.read() } == 0;
    if done && let Some((loader, _)) = loaded() {
        guarded("save", || {
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
        // SAFETY: the game passes one of its player slots, intact on entry.
        let handle: u32 = unsafe { read(slot, 0) };
        if handle != 0
            && let Some(joined) = JOINED.get(index).filter(|_| index < offsets.slot_count)
            && joined.load(Ordering::Relaxed) == handle
        {
            joined.store(0, Ordering::Relaxed);
            let data = player(index, slot, offsets);
            guarded("player removal", || {
                loader.emit(Event::PLAYER_LEFT, None, (&raw const data).cast());
            });
        }
    }
    // SAFETY: forwarded unchanged.
    unsafe { next::<RemoveFn>(&REMOVE_NEXT)(server, slot) };
}

fn queue(data: BuildingData) {
    guarded("building", || {
        BUILDING
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(data);
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
    // SAFETY: forwarded unchanged.
    let entity = unsafe {
        next::<PlaceFn>(&PLACE_NEXT)(
            context, template, position, rotation, item, variant, material,
        )
    };
    if entity != 0
        && !context.is_null()
        && let Some((_, offsets)) = loaded()
    {
        queue(BuildingData {
            header: Header::of::<BuildingData>(),
            entity,
            // SAFETY: the build context holds the acting player and the replay flag.
            player: unsafe { read(context, offsets.ctx_player) },
            item,
            kind: BuildingData::PLACED,
            // SAFETY: as above.
            replay: unsafe { read(context, offsets.ctx_replay) },
            // SAFETY: the game passes its template GUID and position.
            template: unsafe { template.as_ref() }.copied().unwrap_or_default(),
            // SAFETY: as above.
            position: unsafe { position.as_ref() }.copied().unwrap_or_default(),
        });
    }
    entity
}

unsafe extern "system" fn pickup(context: *mut u8, target: *const u8, tool: u32) -> u8 {
    // The target is the entity and the mode; read them while the entity lives.
    let before = loaded()
        .filter(|_| !context.is_null() && !target.is_null())
        .map(|(_, offsets)| {
            // SAFETY: the target starts with the entity id, then the mode byte.
            let entity: u32 = unsafe { read(target, 0) };
            // SAFETY: as above.
            let mode: u8 = unsafe { read(target, 4) };
            // SAFETY: the build context holds the acting player and the replay flag.
            let player: u32 = unsafe { read(context, offsets.ctx_player) };
            // SAFETY: as above.
            let replay: u8 = unsafe { read(context, offsets.ctx_replay) };
            (entity, mode, player, replay)
        });
    // SAFETY: forwarded unchanged.
    let done = unsafe { next::<PickupFn>(&PICKUP_NEXT)(context, target, tool) };
    if done != 0
        && let Some((entity, mode, player, replay)) = before
    {
        queue(BuildingData {
            header: Header::of::<BuildingData>(),
            entity,
            player,
            item: 0,
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
    fn a_panic_in_a_link_s_work_turns_the_events_off_and_stays_inside() {
        let mut ran = false;
        guarded("test", || ran = true);
        assert!(ran && !OFF.load(Ordering::Relaxed));
        guarded("test", || panic!("a loader bug"));
        assert!(OFF.load(Ordering::Relaxed));
        assert!(loaded().is_none());
    }

    #[test]
    fn a_session_follows_its_server_and_thread() {
        let mut session = Session::default();
        assert_eq!(session.step(0x1000, 7), Step::New);
        assert_eq!(session.step(0x1000, 7), Step::Same);
        session.stop();
        assert_eq!(session.step(0x1000, 7), Step::Ended);
        // The next hosted game: a new server on a new thread.
        assert_eq!(session.step(0x2000, 9), Step::New);
        assert_eq!(session.step(0x2000, 9), Step::Same);
        // The same address again, on another thread, is another game.
        session.stop();
        assert_eq!(session.step(0x2000, 11), Step::New);
    }
}
