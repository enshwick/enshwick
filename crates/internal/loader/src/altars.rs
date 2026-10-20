//! The Flame Altars of the server's world, for the mods that opened them.
//!
//! The engine runs a system once per batch of entities, several times a
//! tick, so no pass sees every altar: passes merge into what is known, and an
//! altar no pass has seen for [`FORGET_AFTER`] is forgotten. A mod that
//! controls the zones gets `upgrade_building_range` replaced by vanilla's own
//! loop on the same engine helpers, writing its sizes.

mod geometry;

use core::ffi::c_void;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::TryLockError;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use arc_swap::ArcSwapOption;
use arrayvec::ArrayVec;
use enshwick_loader_api::abi::{
    AltarInfo, AltarZoneSize, Bounds, Event, Header, HookFlags, Side, Status, ZoneCheck,
    ZoneCheckFn,
};
use enshwick_loader_api::altars::MAX_ZONES;

use self::geometry::Placed;
use crate::chains::Next;
use crate::ecs::{Engine, Query, read_at};
use crate::host::{Loader, Registered};

const FORGET_AFTER: Duration = Duration::from_secs(2);
/// `add_buildzones_server`'s iterator: context, `BaseIdComponent`,
/// `CurrentTransform`, `TriggerShape`, `BuildZoneSystem`.
/// Found on the 1.0 build 1269579, server and client.
const ZONES_WORDS: usize = 5;
const ZONES_TRANSFORM: usize = 2;
const ZONES_SHAPE: usize = 3;
/// `CurrentTransform`: position at 0, orientation at 0x18.
/// Found on the 1.0 build 1269579, server and client.
const TRANSFORM_ORIENTATION: usize = 0x18;
/// `upgrade_building_range`'s iterator: context, `Level`, `TriggerShape`, two
/// stamps, entity record, type and flags, tracker, `BalancingRegistry`,
/// `AttributeRegistry`.
/// Found on the 1.0 build 1269579, server and client.
const UPGRADE_WORDS: usize = 10;
const UPGRADE_LEVEL: usize = 1;
const UPGRADE_SHAPE: usize = 2;
const UPGRADE_STAMP_A: usize = 3;
const UPGRADE_STAMP_B: usize = 4;
const UPGRADE_ENTITY: usize = 5;
const UPGRADE_KIND: usize = 6;
const UPGRADE_TRACKER: usize = 7;
const UPGRADE_BALANCING: usize = 8;
const UPGRADE_ATTRIBUTES: usize = 9;
/// `TriggerShape`: half extents at 4.
/// Found on the 1.0 build 1269579, server and client.
const SHAPE_RANGE: usize = 4;
/// Vanilla writes half the level's size.
/// Found on the 1.0 build 1269579, server and client.
const HALF: f32 = 0.5;
/// The most levels an altar reports; the engine has far fewer.
const MAX_LEVELS: usize = 16;

type Sizes = ArrayVec<[f32; 3], MAX_LEVELS>;

#[derive(Debug)]
struct Seen<T> {
    entries: BTreeMap<u32, (T, Instant)>,
}

impl<T: PartialEq> Seen<T> {
    const fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
        }
    }

    /// Records one batch and forgets what nobody saw lately; true when the
    /// known values changed.
    fn merge(&mut self, batch: impl IntoIterator<Item = (u32, T)>, now: Instant) -> bool {
        let mut changed = false;
        for (id, value) in batch {
            if let Some((known, seen)) = self.entries.get_mut(&id) {
                changed |= *known != value;
                *known = value;
                *seen = now;
            } else {
                changed = true;
                self.entries.insert(id, (value, now));
            }
        }
        let before = self.entries.len();
        self.entries
            .retain(|_, (_, seen)| now.duration_since(*seen) < FORGET_AFTER);
        changed || self.entries.len() != before
    }

    fn get(&self, id: u32) -> Option<&T> {
        self.entries.get(&id).map(|(value, _)| value)
    }

    fn iter(&self) -> impl Iterator<Item = (u32, &T)> {
        self.entries.iter().map(|(&id, (value, _))| (id, value))
    }
}

#[derive(Debug)]
struct State {
    placed: Seen<Placed>,
    levels: Seen<Sizes>,
    built: Seen<[f32; 3]>,
}

static STATE: Mutex<State> = Mutex::new(State {
    placed: Seen::new(),
    levels: Seen::new(),
    built: Seen::new(),
});

type Targets = BTreeMap<u32, (usize, [f32; 3])>;

static WANTED: AtomicBool = AtomicBool::new(false);
static OPENERS: Mutex<Vec<usize>> = Mutex::new(Vec::new());
/// The index of the mod that controls the zones, plus one; 0 for none.
static CONTROLLER: AtomicUsize = AtomicUsize::new(0);
static TARGETS: ArcSwapOption<Targets> = ArcSwapOption::const_empty();
static CHANGED: AtomicBool = AtomicBool::new(false);
/// Set when the loader's own work in a link went wrong: from then on the
/// links run vanilla.
static OFF: AtomicBool = AtomicBool::new(false);
// SAFETY: each slot joins the chain of an engine system, typed `SystemFn`.
static ADD_ZONES_NEXT: Next<SystemFn> = unsafe { Next::new() };
// SAFETY: as above.
static UPGRADE_NEXT: Next<SystemFn> = unsafe { Next::new() };

type Filter = Registered<ZoneCheckFn>;

#[derive(Debug, Clone, Copy)]
struct SeamLayout {
    entries: usize,
    count: usize,
    entry_size: usize,
    entry_shape: usize,
    entry_ignored: usize,
    corner_size: usize,
}

static FILTERS: Mutex<Vec<Filter>> = Mutex::new(Vec::new());
static SEALED: OnceLock<Box<[Filter]>> = OnceLock::new();
static SEAM: OnceLock<SeamLayout> = OnceLock::new();
// SAFETY: the slot joins the chain of the seam check, typed `SeamFn`.
static SEAM_NEXT: Next<SeamFn> = unsafe { Next::new() };

thread_local! {
    static PLACED: RefCell<Vec<(u32, Placed)>> = const { RefCell::new(Vec::new()) };
    static BUILT: RefCell<Vec<(u32, [f32; 3])>> = const { RefCell::new(Vec::new()) };
    static LEVELS: RefCell<Vec<(u32, Sizes)>> = const { RefCell::new(Vec::new()) };
}

type SystemFn = unsafe extern "system" fn(ctx: *mut u64);
type SeamFn = unsafe extern "system" fn(zones: *const u8, corners: *const i64, margin: f32) -> u8;

/// Opens altars for mod `owner`; during init only.
pub(crate) fn open(loader: &Loader, owner: usize) -> Status {
    if loader.delivery.get().is_some() {
        return Status::TOO_LATE;
    }
    if crate::ecs::engine(loader).is_none() {
        return Status::MISSING;
    }
    let mut openers = OPENERS.lock().unwrap_or_else(PoisonError::into_inner);
    if !openers.contains(&owner) {
        openers.push(owner);
    }
    WANTED.store(true, Ordering::Release);
    Status::OK
}

/// Makes mod `owner` the one that sizes altar zones; during init only.
pub(crate) fn control(loader: &Loader, owner: usize) -> Status {
    let status = open(loader, owner);
    if !status.is_ok() {
        return status;
    }
    match CONTROLLER.compare_exchange(0, owner + 1, Ordering::AcqRel, Ordering::Acquire) {
        Ok(_) => Status::OK,
        Err(held) if held == owner + 1 => Status::OK,
        Err(_) => Status::REFUSED,
    }
}

pub(crate) fn opened(owner: usize) -> bool {
    OPENERS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .contains(&owner)
}

pub(crate) fn set_sizes(owner: usize, sizes: &[AltarZoneSize]) -> Status {
    if CONTROLLER.load(Ordering::Acquire) != owner + 1 {
        return Status::REFUSED;
    }
    let state = STATE.lock().unwrap_or_else(PoisonError::into_inner);
    let mut targets = Targets::new();
    for size in sizes {
        let (Some(placed), Some(levels)) =
            (state.placed.get(size.altar), state.levels.get(size.altar))
        else {
            continue;
        };
        let level = usize::from(size.level);
        let (Some(turn), Some(current)) = (geometry::turn(placed.orientation), levels.last())
        else {
            continue;
        };
        if levels.len() != level || size.grow < 0 || size.height <= 0 {
            continue;
        }
        let range = current.map(|v| v * HALF);
        targets.insert(
            size.altar,
            (
                level,
                geometry::grown_range(range, turn, size.grow, size.height),
            ),
        );
    }
    drop(state);
    TARGETS.store(Some(Arc::new(targets)));
    Status::OK
}

pub(crate) fn list() -> Vec<AltarInfo> {
    let state = STATE.lock().unwrap_or_else(PoisonError::into_inner);
    state
        .placed
        .iter()
        .map(|(id, placed)| info(id, placed, state.built.get(id), state.levels.get(id)))
        .collect()
}

fn info(id: u32, placed: &Placed, built: Option<&[f32; 3]>, levels: Option<&Sizes>) -> AltarInfo {
    let mut altar = AltarInfo::EMPTY;
    altar.entity = id;
    altar.position = enshwick_loader_api::ecs::metres(placed.position);
    if let Some(levels) = levels {
        altar.level = u8::try_from(levels.len()).unwrap_or(u8::MAX);
    }
    let (Some(turn), Some(centre)) = (geometry::turn(placed.orientation), geometry::centre(placed))
    else {
        altar.tilted = 1;
        return altar;
    };
    if let Some(&range) = built {
        altar.zone = geometry::cells(centre, range, turn);
    }
    if let Some(levels) = levels.filter(|l| l.len() <= MAX_LEVELS) {
        for (slot, size) in altar.levels.iter_mut().zip(levels) {
            *slot = geometry::cells(centre, size.map(|v| v * HALF), turn);
        }
        altar.level_count = u8::try_from(levels.len()).unwrap_or(0);
    }
    altar
}

pub(crate) fn after_step(loader: &Loader) {
    if CHANGED.swap(false, Ordering::AcqRel) {
        loader.emit(Event::ALTARS_CHANGED, None, core::ptr::null());
    }
}

pub(crate) fn forget() {
    let mut state = STATE.lock().unwrap_or_else(PoisonError::into_inner);
    state.placed = Seen::new();
    state.levels = Seen::new();
    state.built = Seen::new();
    drop(state);
    TARGETS.store(None);
    CHANGED.store(true, Ordering::Release);
}

pub(crate) fn register(owner: usize, callback: ZoneCheckFn, user: *mut c_void) -> Status {
    if SEALED.get().is_some() {
        return Status::TOO_LATE;
    }
    FILTERS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(Registered::new(owner, callback, user));
    Status::OK
}

pub(crate) fn link(loader: &'static Loader) {
    link_seam(loader);
    if !WANTED.load(Ordering::Acquire) {
        return;
    }
    if crate::ecs::engine(loader).is_none() {
        crate::log!(WARN, "altars off: this build lacks their engine helpers");
        return;
    }
    let controlled = CONTROLLER.load(Ordering::Acquire) != 0;
    let upgrade_flags = if controlled {
        HookFlags::EXCLUSIVE
    } else {
        HookFlags::NONE
    };
    for (system, next, detour, flags) in [
        (
            "add_buildzones_server",
            &ADD_ZONES_NEXT,
            add_zones as SystemFn,
            HookFlags::NONE,
        ),
        (
            "upgrade_building_range",
            &UPGRADE_NEXT,
            upgrade as SystemFn,
            upgrade_flags,
        ),
    ] {
        let entry = match loader.reflection.system(system) {
            Ok(found) if found.entry() != 0 => found.entry(),
            _ => {
                crate::log!(WARN, "altars off: no system {system} in this build");
                OFF.store(true, Ordering::Release);
                return;
            }
        };
        let status = loader.link(entry, next, detour, flags);
        if !status.is_ok() {
            crate::log!(WARN, "altars off: {system} refused ({})", status.0);
            OFF.store(true, Ordering::Release);
            return;
        }
    }
    crate::log!(
        INFO,
        "altars followed{}",
        if controlled {
            ", zones sized by a mod"
        } else {
            ""
        }
    );
}

fn link_seam(loader: &'static Loader) {
    let filters = std::mem::take(&mut *FILTERS.lock().unwrap_or_else(PoisonError::into_inner));
    let wanted = !filters.is_empty();
    let _first = SEALED.set(filters.into_boxed_slice());
    if !wanted {
        return;
    }
    let size = |name: &str| loader.offset(name);
    let layout = (|| {
        Some(SeamLayout {
            entries: size("building.zone_list_entries")?,
            count: size("building.zone_list_count")?,
            entry_size: size("building.zone_entry_size")?,
            entry_shape: size("building.zone_entry_shape")?,
            entry_ignored: size("building.zone_entry_ignored")?,
            corner_size: size("building.corner_size")?,
        })
    })();
    let (Some(layout), Some(target)) = (layout, loader.address("building.seam_check")) else {
        crate::log!(WARN, "zone checks off: this build lacks the zone list");
        return;
    };
    let _first = SEAM.set(layout);
    let status = loader.link(target, &SEAM_NEXT, seam, HookFlags::NONE);
    if status.is_ok() {
        crate::log!(INFO, "zone checks on");
    } else {
        crate::log!(WARN, "zone checks off: refused ({})", status.0);
    }
}

fn turn_off(why: &str) {
    if !OFF.swap(true, Ordering::AcqRel) {
        crate::log!(WARN, "altars off, vanilla from now on: {why}");
    }
}

/// Whether `ctx` is a context of the server's world.
///
/// # Safety
///
/// `ctx` is a live system context.
unsafe fn server_world(loader: &Loader, engine: &Engine, ctx: *mut u64) -> bool {
    if loader.side == Side::SERVER {
        return true;
    }
    // SAFETY: forwarded from the caller.
    unsafe { engine.is_world(ctx, crate::world::server_world()) }
}

unsafe extern "system" fn add_zones(ctx: *mut u64) {
    // SAFETY: the original system takes its own context.
    unsafe { ADD_ZONES_NEXT.get()(ctx) };
    if OFF.load(Ordering::Acquire) {
        return;
    }
    let (Some(loader), Some(engine)) = (crate::host::loader(), crate::ecs::resolved()) else {
        return;
    };
    let run = || {
        // SAFETY: the system's live context.
        if unsafe { server_world(loader, engine, ctx) } {
            // SAFETY: as above, after vanilla ran.
            unsafe { observe(engine, ctx) };
        }
    };
    if std::panic::catch_unwind(run).is_err() {
        turn_off("the position pass panicked");
    }
}

unsafe extern "system" fn upgrade(ctx: *mut u64) {
    let ready = (!OFF.load(Ordering::Acquire))
        .then(|| crate::host::loader().zip(crate::ecs::resolved()))
        .flatten()
        // SAFETY: the system's live context.
        .filter(|(loader, engine)| unsafe { server_world(loader, engine, ctx) });
    let Some((_, engine)) = ready else {
        // SAFETY: the original system takes its own context.
        unsafe { UPGRADE_NEXT.get()(ctx) };
        return;
    };
    let controller = CONTROLLER.load(Ordering::Acquire);
    if controller != 0 && !crate::host::is_off(controller - 1) {
        // SAFETY: the system's live context.
        if std::panic::catch_unwind(|| unsafe { write_ranges(engine, ctx) }).is_ok() {
            return;
        }
        turn_off("the range pass panicked");
        // Hand the tick to vanilla from the first row.
        // SAFETY: the system's live context.
        unsafe { engine.set_row(ctx, 0) };
        // SAFETY: the original system takes its own context.
        unsafe { UPGRADE_NEXT.get()(ctx) };
        return;
    }
    // SAFETY: the original system takes its own context.
    unsafe { UPGRADE_NEXT.get()(ctx) };
    // SAFETY: the system's live context, after vanilla ran.
    if std::panic::catch_unwind(|| unsafe { read_levels(engine, ctx) }).is_err() {
        turn_off("the level pass panicked");
    }
}

/// How many entries a thread holds back while the state is busy before it
/// waits for the state instead.
const HELD_BACK: usize = 4096;

/// Merges this thread's batches into the state. When another pass holds the
/// state, the batches stay queued in the thread's buffers and go in with
/// its next pass, so no pass is lost; a queue grown past [`HELD_BACK`] waits
/// for the state.
fn merge(
    placed: &mut Vec<(u32, Placed)>,
    built: &mut Vec<(u32, [f32; 3])>,
    levels: &mut Vec<(u32, Sizes)>,
) {
    let mut state = match STATE.try_lock() {
        Ok(state) => state,
        Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
        Err(TryLockError::WouldBlock) if placed.len() + built.len() + levels.len() < HELD_BACK => {
            return;
        }
        Err(TryLockError::WouldBlock) => STATE.lock().unwrap_or_else(PoisonError::into_inner),
    };
    let now = Instant::now();
    let mut changed = false;
    if !placed.is_empty() {
        changed |= state.placed.merge(placed.drain(..), now);
    }
    if !built.is_empty() {
        changed |= state.built.merge(built.drain(..), now);
    }
    if !levels.is_empty() {
        changed |= state.levels.merge(levels.drain(..), now);
    }
    if changed {
        CHANGED.store(true, Ordering::Release);
    }
}

/// Learns positions, orientations and built zones, after vanilla built them.
///
/// # Safety
///
/// `ctx` is the live context of `add_buildzones_server`.
unsafe fn observe(engine: &Engine, ctx: *mut u64) {
    PLACED.with_borrow_mut(|placed| {
        BUILT.with_borrow_mut(|built| {
            LEVELS.with_borrow_mut(|levels| {
                // SAFETY: forwarded from the caller.
                unsafe { observe_into(engine, ctx, placed, built) };
                merge(placed, built, levels);
            });
        });
    });
}

/// # Safety
///
/// `ctx` is the live context of `add_buildzones_server`.
unsafe fn observe_into(
    engine: &Engine,
    ctx: *mut u64,
    placed: &mut Vec<(u32, Placed)>,
    built: &mut Vec<(u32, [f32; 3])>,
) {
    // Vanilla left the row counter at the end: walk again from the start and
    // restore it.
    // SAFETY: the system's live context.
    let saved = unsafe { engine.row(ctx) };
    // SAFETY: as above.
    unsafe { engine.set_row(ctx, 0) };
    // SAFETY: as above, with the system's iterator size.
    let mut query: Query<'_, ZONES_WORDS> = unsafe { engine.query(ctx) };
    while let Some(words) = query.next() {
        let transform = words[ZONES_TRANSFORM] as *const u8;
        let shape = words[ZONES_SHAPE] as *const u8;
        if transform.is_null() || shape.is_null() {
            continue;
        }
        // SAFETY: the system's live context.
        let id = unsafe { engine.entity(ctx) };
        // SAFETY: the row's trigger shape holds its half extents here.
        let range: [f32; 3] = unsafe { read_at(shape, SHAPE_RANGE) };
        // SAFETY: the row's transform holds its position at 0.
        let position: [i64; 3] = unsafe { read_at(transform, 0) };
        // SAFETY: and its orientation here.
        let orientation: [f32; 4] = unsafe { read_at(transform, TRANSFORM_ORIENTATION) };
        built.push((id, range));
        placed.push((
            id,
            Placed {
                position,
                orientation,
            },
        ));
    }
    // SAFETY: the system's live context.
    unsafe { engine.set_row(ctx, saved) };
}

/// Learns levels after vanilla's `upgrade_building_range`.
///
/// # Safety
///
/// `ctx` is the live context of `upgrade_building_range`.
unsafe fn read_levels(engine: &Engine, ctx: *mut u64) {
    LEVELS.with_borrow_mut(|levels| {
        // SAFETY: forwarded from the caller.
        unsafe { read_levels_into(engine, ctx, levels) };
        PLACED.with_borrow_mut(|placed| {
            BUILT.with_borrow_mut(|built| merge(placed, built, levels));
        });
    });
}

/// # Safety
///
/// `ctx` is the live context of `upgrade_building_range`.
unsafe fn read_levels_into(engine: &Engine, ctx: *mut u64, levels: &mut Vec<(u32, Sizes)>) {
    // SAFETY: the system's live context.
    let saved = unsafe { engine.row(ctx) };
    // SAFETY: as above.
    unsafe { engine.set_row(ctx, 0) };
    // SAFETY: as above, with the system's iterator size.
    let mut query: Query<'_, UPGRADE_WORDS> = unsafe { engine.query(ctx) };
    while let Some(words) = query.next() {
        let words = *words;
        // SAFETY: the words of the current row.
        let Some(level) = (unsafe { altar_level(engine, &words) }) else {
            continue;
        };
        // SAFETY: as above.
        let Some((_, sizes)) = (unsafe { level_sizes(engine, &words, level) }) else {
            continue;
        };
        // SAFETY: the system's live context.
        levels.push((unsafe { engine.entity(ctx) }, sizes));
    }
    // SAFETY: the system's live context.
    unsafe { engine.set_row(ctx, saved) };
}

/// Vanilla's `upgrade_building_range`, writing the controlling mod's ranges
/// where it has one for the altar's level.
///
/// # Safety
///
/// `ctx` is the live context of `upgrade_building_range`.
unsafe fn write_ranges(engine: &Engine, ctx: *mut u64) {
    LEVELS.with_borrow_mut(|levels| {
        // SAFETY: forwarded from the caller.
        unsafe { write_ranges_into(engine, ctx, levels) };
        merge(&mut Vec::new(), &mut Vec::new(), levels);
    });
}

/// # Safety
///
/// `ctx` is the live context of `upgrade_building_range`.
unsafe fn write_ranges_into(engine: &Engine, ctx: *mut u64, levels: &mut Vec<(u32, Sizes)>) {
    let targets = TARGETS.load();
    // SAFETY: the system's live context.
    let mut query: Query<'_, UPGRADE_WORDS> = unsafe { engine.query(ctx) };
    while let Some(words) = query.next() {
        let words = *words;
        // SAFETY: the words of the current row.
        let Some(level) = (unsafe { altar_level(engine, &words) }) else {
            continue;
        };
        // SAFETY: the balancing registry of the current row.
        let Some((current, sizes)) = (unsafe { level_sizes(engine, &words, level) }) else {
            continue;
        };
        // SAFETY: the system's live context.
        let id = unsafe { engine.entity(ctx) };
        let target = match targets.as_deref().and_then(|targets| targets.get(&id)) {
            Some(&(solved_level, range)) if solved_level == level => range,
            _ => current.map(|v| v * HALF),
        };
        levels.push((id, sizes));
        let shape = words[UPGRADE_SHAPE] as *mut u8;
        if shape.is_null() {
            continue;
        }
        // SAFETY: the trigger shape this system writes holds its range here.
        let written: [f32; 3] = unsafe { read_at(shape, SHAPE_RANGE) };
        if written.map(f32::to_bits) != target.map(f32::to_bits) {
            // SAFETY: the change-tracking words of the current row.
            unsafe {
                engine.mark_changed(
                    words[UPGRADE_TRACKER],
                    words[UPGRADE_STAMP_A],
                    words[UPGRADE_STAMP_B],
                    words[UPGRADE_ENTITY],
                    words[UPGRADE_KIND],
                );
            }
            // SAFETY: as above.
            unsafe {
                shape
                    .wrapping_add(SHAPE_RANGE)
                    .cast::<[f32; 3]>()
                    .write_unaligned(target);
            }
        }
    }
}

/// The altar level of the current row, or `None` for a row that is no altar.
///
/// # Safety
///
/// `words` are the current row's iterator words.
unsafe fn altar_level(engine: &Engine, words: &[u64; UPGRADE_WORDS]) -> Option<usize> {
    let level_component = words[UPGRADE_LEVEL] as *const u8;
    // SAFETY: the row's `Level` component and attribute registry.
    let value = unsafe { engine.level(words[UPGRADE_ATTRIBUTES], level_component) }?;
    // The level is the attribute's low byte.
    Some(usize::from(value.to_le_bytes()[0]))
}

/// The current level's size and the sizes of every level up to it; `None`
/// when a size is missing.
///
/// # Safety
///
/// `words` are the current row's iterator words.
unsafe fn level_sizes(
    engine: &Engine,
    words: &[u64; UPGRADE_WORDS],
    level: usize,
) -> Option<([f32; 3], Sizes)> {
    if level > MAX_LEVELS {
        return None;
    }
    // SAFETY: the balancing registry of the row.
    let size_of = |l: usize| unsafe { engine.zone_size(words[UPGRADE_BALANCING], l) };
    let current = size_of(level)?;
    let mut sizes = Sizes::new();
    for l in 1..=level {
        sizes.try_push(size_of(l)?).ok()?;
    }
    Some((current, sizes))
}

unsafe extern "system" fn seam(zones: *const u8, corners: *const i64, margin: f32) -> u8 {
    // SAFETY: the rest of the chain takes the arguments the game passed.
    let vanilla = unsafe { SEAM_NEXT.get()(zones, corners, margin) };
    if vanilla != 0 || zones.is_null() || corners.is_null() {
        return vanilla;
    }
    let (Some(layout), Some(filters)) = (SEAM.get(), SEALED.get()) else {
        return vanilla;
    };
    // SAFETY: the game passes its zone list and an object's two corners.
    let accepted =
        std::panic::catch_unwind(|| unsafe { check(layout, filters, zones, corners, margin) });
    u8::from(accepted.unwrap_or(false))
}

/// Asks the filters about a placement vanilla refused.
///
/// # Safety
///
/// `list` is the engine's zone list and `corners` two corner points.
#[expect(clippy::cast_possible_truncation, reason = "margins are a few metres")]
unsafe fn check(
    layout: &SeamLayout,
    filters: &[Filter],
    list: *const u8,
    corners: *const i64,
    margin: f32,
) -> bool {
    // SAFETY: the list holds its entries pointer and count at these offsets.
    let entries: *const u8 = unsafe { read_at(list, layout.entries) };
    // SAFETY: as above.
    let count: usize = unsafe { read_at(list, layout.count) };
    if entries.is_null() || count > MAX_ZONES {
        return false;
    }
    let mut zones: ArrayVec<Bounds, MAX_ZONES> = ArrayVec::new();
    for i in 0..count {
        let entry = entries.wrapping_add(i * layout.entry_size);
        // SAFETY: `i < count` entries of `entry_size` bytes.
        let ignored: u8 = unsafe { read_at(entry, layout.entry_ignored) };
        // SAFETY: as above.
        let shape: u8 = unsafe { read_at(entry, layout.entry_shape) };
        if ignored != 0 || shape != 0 {
            continue;
        }
        // SAFETY: an entry starts with min and max as six i64.
        let bounds: [i64; 6] = unsafe { read_at(entry, 0) };
        zones.push(Bounds {
            min: [bounds[0], bounds[1], bounds[2]],
            max: [bounds[3], bounds[4], bounds[5]],
        });
    }
    // SAFETY: the game passes two corners of three i64 each.
    let low: [i64; 3] = unsafe { read_at(corners.cast(), 0) };
    // SAFETY: as above.
    let high: [i64; 3] = unsafe { read_at(corners.cast(), layout.corner_size) };
    let call = ZoneCheck {
        header: Header::of::<ZoneCheck>(),
        zones: zones.as_ptr(),
        zone_count: zones.len(),
        object: Bounds {
            min: [0, 1, 2].map(|a| low[a].min(high[a])),
            max: [0, 1, 2].map(|a| low[a].max(high[a])),
        },
        margin: (f64::from(margin) * enshwick_loader_api::ecs::FIXED_PER_METRE) as i64,
    };
    filters.iter().filter(|f| f.live()).any(|f| {
        // SAFETY: the callback lives in the mod's DLL, which stays loaded,
        // and takes the pointer it registered; the call is valid for it.
        crate::profile::call(f.owner, crate::profile::Feature::Altars, || unsafe {
            (f.callback)(&raw const call, f.user()) != 0
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seen_merges_batches_and_reports_changes() {
        let now = Instant::now();
        let mut seen: Seen<u8> = Seen::new();
        assert!(seen.merge([(1, 10), (2, 20)], now));
        assert!(!seen.merge([(1, 10)], now));
        assert!(seen.merge([(2, 21)], now));
        assert_eq!(seen.get(2), Some(&21));
        assert_eq!(seen.iter().count(), 2);
        assert!(seen.merge([], now + FORGET_AFTER));
        assert_eq!(seen.iter().count(), 0);
    }

    #[test]
    fn an_upright_altar_reports_its_zone_and_levels() {
        let placed = Placed {
            position: [
                3636_i64 << 32 | 3 << 30,
                860_i64 << 32 | 1 << 31,
                1339_i64 << 32 | 3 << 30,
            ],
            orientation: [0.0, 0.0, 0.0, 1.0],
        };
        let altar = info(
            7,
            &placed,
            Some(&[20.0; 3]),
            Some(&[[40.0; 3]].into_iter().collect()),
        );
        assert_eq!(altar.entity, 7);
        assert_eq!(altar.level, 1);
        assert_eq!(altar.tilted, 0);
        assert_eq!(
            altar.zone,
            Bounds {
                min: [7234, 1681, 2640],
                max: [7314, 1761, 2720]
            }
        );
        assert_eq!(altar.level_count, 1);
        assert_eq!(altar.levels[0], altar.zone);
    }

    #[test]
    fn a_tilted_altar_has_no_zone() {
        let placed = Placed {
            position: [0; 3],
            orientation: [0.3, 0.0, 0.0, 0.95],
        };
        let altar = info(7, &placed, Some(&[20.0; 3]), None);
        assert_eq!(altar.tilted, 1);
        assert_eq!(altar.zone, Bounds::default());
    }
}
