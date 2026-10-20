//! Game events on the server, raised from the loader's own links: tick, world
//! loaded and saved, player joined and left, shutdown. A link is made only
//! when a mod subscribed to one of its events.

use core::ffi::c_void;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering};

use enshwick_api::abi::{Event, Header, HookFlags, PlayerData, TickData};

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
}

static LOADED: OnceLock<(&'static Loader, Offsets)> = OnceLock::new();
static STEP_NEXT: AtomicUsize = AtomicUsize::new(0);
static STATE_NEXT: AtomicUsize = AtomicUsize::new(0);
static SAVE_NEXT: AtomicUsize = AtomicUsize::new(0);
static REMOVE_NEXT: AtomicUsize = AtomicUsize::new(0);
static STEPS: AtomicU64 = AtomicU64::new(0);
/// The handle of the player seen joining in each slot, 0 for none.
static JOINED: [AtomicU32; MAX_SLOTS] = [const { AtomicU32::new(0) }; MAX_SLOTS];

type StepFn = unsafe extern "system" fn(server: *mut u8);
type StateFn = unsafe extern "system" fn(entry: *mut u8, state: u8);
type SaveFn = unsafe extern "system" fn(out: *mut u8, save: *mut u8, context: *mut u8) -> *mut u8;
type RemoveFn = unsafe extern "system" fn(server: *mut u8, slot: *mut u8);

/// Links what the subscriptions need; a missing symbol leaves its events
/// silent and says so.
pub(crate) fn link(loader: &'static Loader) {
    let wants = |events: &[Event]| events.iter().any(|&e| loader.subscribed(e));
    if !wants(&[
        Event::TICK,
        Event::WORLD_LOADED,
        Event::WORLD_SAVED,
        Event::PLAYER_JOINED,
        Event::PLAYER_LEFT,
        Event::SHUTDOWN,
    ]) {
        return;
    }
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
        })
    })();
    let Some(offsets) = offsets else {
        crate::log!(WARN, "game events off: this build has no server layout");
        return;
    };
    if LOADED.set((loader, offsets)).is_err() {
        return;
    }
    let links: [(&[Event], &str, *const (), &'static AtomicUsize); 4] = [
        (
            &[Event::TICK, Event::PLAYER_JOINED],
            "server.post_step",
            step as *const (),
            &STEP_NEXT,
        ),
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
    ];
    for (events, symbol, detour, next) in links {
        if !wants(events) {
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
    let Some((loader, offsets)) = LOADED.get() else {
        return;
    };
    let step = STEPS.fetch_add(1, Ordering::Relaxed) + 1;
    let tick = TickData {
        header: Header::of::<TickData>(),
        step,
    };
    loader.emit(Event::TICK, None, (&raw const tick).cast::<c_void>());
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

unsafe extern "system" fn set_state(entry: *mut u8, state: u8) {
    let old = LOADED
        .get()
        .filter(|_| !entry.is_null())
        // SAFETY: the entry is the game's server instance entry.
        .map(|(_, offsets)| unsafe { read::<u8>(entry, offsets.entry_state) });
    // SAFETY: forwarded unchanged.
    unsafe { next::<StateFn>(&STATE_NEXT)(entry, state) };
    let (Some((loader, _)), Some(old)) = (LOADED.get(), old) else {
        return;
    };
    match state {
        STATE_RUN if old != STATE_RUN => loader.emit(Event::WORLD_LOADED, None, core::ptr::null()),
        STATE_SHUTDOWN if old != STATE_SHUTDOWN => {
            loader.emit(Event::SHUTDOWN, None, core::ptr::null());
        }
        _ => {}
    }
}

unsafe extern "system" fn save(out: *mut u8, save: *mut u8, context: *mut u8) -> *mut u8 {
    // SAFETY: forwarded unchanged.
    let result = unsafe { next::<SaveFn>(&SAVE_NEXT)(out, save, context) };
    // SAFETY: the game wrote its result's first byte through `out`.
    let done = !out.is_null() && unsafe { out.read() } == 0;
    if done && let Some((loader, _)) = LOADED.get() {
        loader.emit(Event::WORLD_SAVED, None, core::ptr::null());
    }
    result
}

unsafe extern "system" fn remove(server: *mut u8, slot: *mut u8) {
    if let Some((loader, offsets)) = LOADED.get()
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
            loader.emit(Event::PLAYER_LEFT, None, (&raw const data).cast());
        }
    }
    // SAFETY: forwarded unchanged.
    unsafe { next::<RemoveFn>(&REMOVE_NEXT)(server, slot) };
}
