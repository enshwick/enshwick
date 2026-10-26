//! The Flame Altars of the server's world, for the mods that opened them.
//!
//! The engine runs a system once per batch of entities, several times a
//! tick, so no pass sees every altar: passes merge into what is known, and an
//! altar no pass has seen for [`FORGET_AFTER`] and [`FORGET_STEPS`] server
//! steps is forgotten, the last one too. Counting steps keeps a server that
//! stalls from forgetting altars it could not step over. A mod that controls
//! the zones gets `upgrade_building_range` replaced by vanilla's own loop on
//! the same engine helpers, writing its sizes.

use crate::locks::Relock;
use crate::registry::Registry;

mod geometry;

use core::ffi::c_void;
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::sync::TryLockError;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use arc_swap::ArcSwapOption;
use arrayvec::ArrayVec;
use enshwick::abi::{
    AltarInfo, AltarZoneSize, Bounds, Event, Header, HookFlags, Side, Status, ZoneCheck,
    ZoneCheckFn,
};
use enshwick::altars::MAX_ZONES;
use enshwick::raw::{read_at, read_ptr_at, write_at};

use self::geometry::Placed;
use crate::chains::Next;
use crate::ecs::{Engine, Query};
use crate::host::{Loader, Registered};

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[
    "building.corner_size",
    "building.seam_check",
    "building.zone_entry_ignored",
    "building.zone_entry_shape",
    "building.zone_entry_size",
    "building.zone_list_count",
    "building.zone_list_entries",
];

const FORGET_AFTER: Duration = Duration::from_secs(2);
/// Server steps an altar stays known without a pass seeing it, together
/// with [`FORGET_AFTER`].
const FORGET_STEPS: u64 = 60;
/// How many steps apart the post-step looks for altars no pass sees.
const EXPIRE_EVERY: u64 = 16;
/// `add_buildzones_server`'s iterator: context, `BaseIdComponent`,
/// `CurrentTransform`, `TriggerShape`, `BuildZoneSystem`.
/// Found on the 1.0 build 1269579, server and client, and checked against
/// the system's reflected components on every build.
const ZONES_READS: [&str; 3] = ["BaseIdComponent", "CurrentTransform", "TriggerShape"];
const ZONES_WORDS: usize = 5;
const ZONES_TRANSFORM: usize = 2;
const ZONES_SHAPE: usize = 3;
/// `upgrade_building_range`'s iterator: context, `Level`, `TriggerShape`, two
/// stamps, entity record, type and flags, tracker, `BalancingRegistry`,
/// `AttributeRegistry`.
/// Found on the 1.0 build 1269579, server and client, and checked against
/// the system's reflected components on every build.
const UPGRADE_READS: [&str; 1] = ["Level"];
const UPGRADE_WRITES: [&str; 1] = ["TriggerShape"];
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
/// Vanilla writes half the level's size.
/// Found on the 1.0 build 1269579, server and client.
const HALF: f32 = 0.5;
/// The most levels an altar reports; the engine has far fewer.
const MAX_LEVELS: usize = 16;

type Sizes = ArrayVec<[f32; 3], MAX_LEVELS>;

/// When a pass last saw an entry: the time and the server step.
#[derive(Clone, Copy, Debug)]
struct When {
    at: Instant,
    step: u64,
}

impl When {
    /// Whether an entry seen at `self` is still known at `now`: forgotten
    /// only once both the time and the steps ran out.
    fn fresh(self, now: Self) -> bool {
        now.at.duration_since(self.at) < FORGET_AFTER
            || now.step.saturating_sub(self.step) < FORGET_STEPS
    }
}

#[derive(Debug)]
struct Seen<T> {
    entries: BTreeMap<u32, (T, When)>,
}

impl<T: PartialEq> Seen<T> {
    const fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
        }
    }

    /// Records one batch and forgets what nobody saw lately; true when the
    /// known values changed.
    fn merge(&mut self, batch: impl IntoIterator<Item = (u32, T)>, now: When) -> bool {
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
        self.expire(now) || changed
    }

    /// Forgets what nobody saw lately; true when anything went.
    fn expire(&mut self, now: When) -> bool {
        let before = self.entries.len();
        self.entries.retain(|_, (_, seen)| seen.fresh(now));
        self.entries.len() != before
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
static FIELDS: OnceLock<Fields> = OnceLock::new();

/// Where a trigger shape keeps its half extents and a current transform its
/// position and orientation, from reflection.
#[derive(Debug, Clone, Copy)]
struct Fields {
    range: usize,
    position: usize,
    orientation: usize,
}

impl Fields {
    fn read(loader: &Loader) -> Result<Self, String> {
        let field = |ty: &str, name: &str| {
            loader
                .reflection
                .field_offset(ty, name)
                .ok()
                .and_then(|v| usize::try_from(v).ok())
                .ok_or(format!("no {ty}.{name} in this build"))
        };
        let transform = field("keen::ecs::CurrentTransform", "transform")?;
        Ok(Self {
            range: field("keen::ecs::TriggerShape", "rangeX")?,
            position: transform + field("keen::WorldTransform", "position")?,
            orientation: transform + field("keen::WorldTransform", "orientation")?,
        })
    }
}
static OPENERS: Mutex<Vec<usize>> = Mutex::new(Vec::new());
/// The index of the mod that controls the zones, plus one; 0 for none.
static CONTROLLER: AtomicUsize = AtomicUsize::new(0);
static TARGETS: ArcSwapOption<Targets> = ArcSwapOption::const_empty();
static CHANGED: AtomicBool = AtomicBool::new(false);
/// The server steps the post-step counted.
static STEP: AtomicU64 = AtomicU64::new(0);
/// Bumped when the world goes, so threads drop what they held back from it.
static WORLD: AtomicU64 = AtomicU64::new(0);
/// Set when the loader's own work in a link went wrong: from then on the
/// links run vanilla.
static OFF: AtomicBool = AtomicBool::new(false);
// SAFETY: `SystemFn` is the signature of every system entry, here the
// reflected `add_buildzones_server`'s, and `link` uses this slot only as that
// function's next.
static ADD_ZONES_NEXT: Next<SystemFn> = unsafe { Next::new() };
// SAFETY: `SystemFn` is the signature of every system entry, here the
// reflected `upgrade_building_range`'s, and `link` uses this slot only as
// that function's next.
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

static FILTERS: Registry<Filter> = Registry::new();
static SEAM: OnceLock<SeamLayout> = OnceLock::new();
// SAFETY: `SeamFn` is the signature of the build table's
// `building.seam_check`, and `link_seam` uses this slot only as that
// function's next.
static SEAM_NEXT: Next<SeamFn> = unsafe { Next::new() };

thread_local! {
    static PLACED: RefCell<Vec<(u32, Placed)>> = const { RefCell::new(Vec::new()) };
    static BUILT: RefCell<Vec<(u32, [f32; 3])>> = const { RefCell::new(Vec::new()) };
    static LEVELS: RefCell<Vec<(u32, Sizes)>> = const { RefCell::new(Vec::new()) };
    /// The world this thread's buffers hold entries of.
    static BUFFERED_WORLD: Cell<u64> = const { Cell::new(0) };
}

/// Empties this thread's buffers when they hold entries of a world gone
/// since; before a pass adds to them.
fn drop_stale_buffers() {
    let world = WORLD.load(Ordering::Acquire);
    if BUFFERED_WORLD.replace(world) != world {
        PLACED.with_borrow_mut(Vec::clear);
        BUILT.with_borrow_mut(Vec::clear);
        LEVELS.with_borrow_mut(Vec::clear);
    }
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
    let mut openers = OPENERS.relock();
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
    OPENERS.relock().contains(&owner)
}

pub(crate) fn set_sizes(owner: usize, sizes: &[AltarZoneSize]) -> Status {
    if CONTROLLER.load(Ordering::Acquire) != owner + 1 {
        return Status::REFUSED;
    }
    let state = STATE.relock();
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
    let state = STATE.relock();
    state
        .placed
        .iter()
        .map(|(id, placed)| info(id, placed, state.built.get(id), state.levels.get(id)))
        .collect()
}

fn info(id: u32, placed: &Placed, built: Option<&[f32; 3]>, levels: Option<&Sizes>) -> AltarInfo {
    let mut altar = AltarInfo::EMPTY;
    altar.entity = id;
    altar.position = enshwick::ecs::metres(placed.position);
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

/// The time and server step now.
fn now() -> When {
    When {
        at: Instant::now(),
        step: STEP.load(Ordering::Relaxed),
    }
}

pub(crate) fn after_step(loader: &Loader) {
    let step = STEP.fetch_add(1, Ordering::Relaxed) + 1;
    // No pass runs once the last altar is gone, so the post-step forgets.
    if step.is_multiple_of(EXPIRE_EVERY)
        && let Ok(mut state) = STATE.try_lock()
    {
        let now = now();
        let state = &mut *state;
        let gone = state.placed.expire(now) | state.built.expire(now) | state.levels.expire(now);
        if gone {
            CHANGED.store(true, Ordering::Release);
        }
    }
    if CHANGED.swap(false, Ordering::AcqRel) {
        loader.emit(Event::ALTARS_CHANGED, None, core::ptr::null());
    }
}

pub(crate) fn forget() {
    WORLD.fetch_add(1, Ordering::AcqRel);
    let mut state = STATE.relock();
    state.placed = Seen::new();
    state.levels = Seen::new();
    state.built = Seen::new();
    drop(state);
    TARGETS.store(None);
    CHANGED.store(true, Ordering::Release);
}

pub(crate) fn register(owner: usize, callback: ZoneCheckFn, user: *mut c_void) -> Status {
    if FILTERS.is_sealed() {
        return Status::TOO_LATE;
    }
    FILTERS
        .add(Registered::new(owner, callback, user))
        .map_or(Status::TOO_LATE, |()| Status::OK)
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
    let checked = Fields::read(loader).and_then(|fields| {
        crate::ecs::components_match(loader, "add_buildzones_server", &ZONES_READS, &[])?;
        crate::ecs::components_match(
            loader,
            "upgrade_building_range",
            &UPGRADE_READS,
            &UPGRADE_WRITES,
        )?;
        Ok(fields)
    });
    match checked {
        Ok(fields) => {
            let _first = FIELDS.set(fields);
        }
        Err(why) => {
            crate::log!(WARN, "altars off: {why}");
            OFF.store(true, Ordering::Release);
            return;
        }
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
    let filters = FILTERS.seal();
    let wanted = !filters.is_empty();
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
/// - `ctx` is null or a live system context, used on its system's thread.
unsafe fn server_world(loader: &Loader, engine: &Engine, ctx: *mut u64) -> bool {
    if loader.side == Side::SERVER {
        return true;
    }
    // SAFETY: the caller guarantees a null or live system context on its
    // thread, all `is_world` asks for.
    unsafe { engine.is_world(ctx, crate::world::server_world()) }
}

unsafe extern "system" fn add_zones(ctx: *mut u64) {
    // SAFETY: this detour runs only as the `add_buildzones_server` link, so
    // the slot holds the next system entry; the system's own context passes
    // unchanged.
    unsafe { ADD_ZONES_NEXT.get()(ctx) };
    if OFF.load(Ordering::Acquire) {
        return;
    }
    let (Some(loader), Some(engine)) = (crate::host::loader(), crate::ecs::resolved()) else {
        return;
    };
    let run = || {
        // SAFETY: `ctx` is the hooked system's own live context, on its
        // thread.
        if unsafe { server_world(loader, engine, ctx) } {
            // SAFETY: `ctx` is the live context of `add_buildzones_server`,
            // on its thread after vanilla ran.
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
        // SAFETY: `ctx` is the hooked system's own live context, on its
        // thread.
        .filter(|(loader, engine)| unsafe { server_world(loader, engine, ctx) });
    let Some((_, engine)) = ready else {
        // SAFETY: this detour runs only as the `upgrade_building_range` link,
        // so the slot holds the next system entry; the system's own context
        // passes unchanged.
        unsafe { UPGRADE_NEXT.get()(ctx) };
        return;
    };
    let controller = CONTROLLER.load(Ordering::Acquire);
    if controller != 0 && !crate::host::is_off(controller - 1) {
        // SAFETY: `ctx` is the live context of `upgrade_building_range`, on
        // its thread, and vanilla has not run this tick.
        if std::panic::catch_unwind(|| unsafe { write_ranges(engine, ctx) }).is_ok() {
            return;
        }
        turn_off("the range pass panicked");
        // Hand the tick to vanilla from the first row.
        // SAFETY: `ctx` is the hooked system's own live context, which only
        // its thread writes.
        unsafe { engine.set_row(ctx, 0) };
        // SAFETY: this detour runs only as the `upgrade_building_range` link,
        // so the slot holds the next system entry; the system's own context
        // passes unchanged.
        unsafe { UPGRADE_NEXT.get()(ctx) };
        return;
    }
    // SAFETY: this detour runs only as the `upgrade_building_range` link, so
    // the slot holds the next system entry; the system's own context passes
    // unchanged.
    unsafe { UPGRADE_NEXT.get()(ctx) };
    // SAFETY: `ctx` is the live context of `upgrade_building_range`, on its
    // thread after vanilla ran.
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
        Err(TryLockError::WouldBlock) => STATE.relock(),
    };
    let now = now();
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
/// - `ctx` is the live context of `add_buildzones_server`, used on its
///   thread while the system runs.
unsafe fn observe(engine: &Engine, ctx: *mut u64) {
    drop_stale_buffers();
    PLACED.with_borrow_mut(|placed| {
        BUILT.with_borrow_mut(|built| {
            LEVELS.with_borrow_mut(|levels| {
                // SAFETY: the caller guarantees the live context of
                // `add_buildzones_server` on its thread.
                unsafe { observe_into(engine, ctx, placed, built) };
                merge(placed, built, levels);
            });
        });
    });
}

/// Walks `add_buildzones_server`'s rows again into `placed` and `built`.
///
/// # Safety
///
/// - `ctx` is the live context of `add_buildzones_server`, used on its
///   thread while the system runs.
unsafe fn observe_into(
    engine: &Engine,
    ctx: *mut u64,
    placed: &mut Vec<(u32, Placed)>,
    built: &mut Vec<(u32, [f32; 3])>,
) {
    let Some(fields) = FIELDS.get() else {
        return;
    };
    // Vanilla left the row counter at the end: walk again from the start and
    // restore it.
    // SAFETY: the caller guarantees the system's live context on its thread.
    let saved = unsafe { engine.row(ctx) };
    // SAFETY: the caller guarantees the system's live context on its
    // thread, which alone writes it.
    unsafe { engine.set_row(ctx, 0) };
    // SAFETY: the caller guarantees the system's live context on its thread;
    // its iterator is `ZONES_WORDS` words, checked against the system's
    // reflected components at link; the query ends inside this call.
    let mut query: Query<'_, ZONES_WORDS> = unsafe { engine.query(ctx) };
    while let Some(words) = query.next() {
        let transform = words[ZONES_TRANSFORM] as *const u8;
        let shape = words[ZONES_SHAPE] as *const u8;
        if transform.is_null() || shape.is_null() {
            continue;
        }
        // SAFETY: the caller guarantees the system's live context on its
        // thread.
        let id = unsafe { engine.entity(ctx) };
        // SAFETY: `shape` is the row's non-null `TriggerShape` by the check
        // before it; its half extents sit at the reflected `rangeX` field.
        let range: [f32; 3] = unsafe { read_at(shape, fields.range) };
        // SAFETY: `transform` is the row's non-null `CurrentTransform` by
        // the check before it; its position sits at the reflected
        // `transform.position` field.
        let position: [i64; 3] = unsafe { read_at(transform, fields.position) };
        // SAFETY: `transform` is the row's non-null `CurrentTransform` by
        // the check before it; its orientation sits at the reflected
        // `transform.orientation` field.
        let orientation: [f32; 4] = unsafe { read_at(transform, fields.orientation) };
        built.push((id, range));
        placed.push((
            id,
            Placed {
                position,
                orientation,
            },
        ));
    }
    // SAFETY: the caller guarantees the system's live context on its
    // thread, which alone writes it.
    unsafe { engine.set_row(ctx, saved) };
}

/// Learns levels after vanilla's `upgrade_building_range`.
///
/// # Safety
///
/// - `ctx` is the live context of `upgrade_building_range`, used on its
///   thread while the system runs.
unsafe fn read_levels(engine: &Engine, ctx: *mut u64) {
    drop_stale_buffers();
    LEVELS.with_borrow_mut(|levels| {
        // SAFETY: the caller guarantees the live context of
        // `upgrade_building_range` on its thread.
        unsafe { read_levels_into(engine, ctx, levels) };
        PLACED.with_borrow_mut(|placed| {
            BUILT.with_borrow_mut(|built| merge(placed, built, levels));
        });
    });
}

/// Walks `upgrade_building_range`'s rows again into `levels`.
///
/// # Safety
///
/// - `ctx` is the live context of `upgrade_building_range`, used on its
///   thread while the system runs.
unsafe fn read_levels_into(engine: &Engine, ctx: *mut u64, levels: &mut Vec<(u32, Sizes)>) {
    // SAFETY: the caller guarantees the system's live context on its thread.
    let saved = unsafe { engine.row(ctx) };
    // SAFETY: the caller guarantees the system's live context on its
    // thread, which alone writes it.
    unsafe { engine.set_row(ctx, 0) };
    // SAFETY: the caller guarantees the system's live context on its thread;
    // its iterator is `UPGRADE_WORDS` words, checked against the system's
    // reflected components at link; the query ends inside this call.
    let mut query: Query<'_, UPGRADE_WORDS> = unsafe { engine.query(ctx) };
    while let Some(words) = query.next() {
        let words = *words;
        // SAFETY: `words` are the current row's iterator words of the live
        // query, on the system's thread.
        let Some(level) = (unsafe { altar_level(engine, &words) }) else {
            continue;
        };
        // SAFETY: `words` are the current row's iterator words of the live
        // query, on the system's thread.
        let Some((_, sizes)) = (unsafe { level_sizes(engine, &words, level) }) else {
            continue;
        };
        // SAFETY: the caller guarantees the system's live context on its
        // thread.
        levels.push((unsafe { engine.entity(ctx) }, sizes));
    }
    // SAFETY: the caller guarantees the system's live context on its
    // thread, which alone writes it.
    unsafe { engine.set_row(ctx, saved) };
}

/// Vanilla's `upgrade_building_range`, writing the controlling mod's ranges
/// where it has one for the altar's level.
///
/// # Safety
///
/// - `ctx` is the live context of `upgrade_building_range`, used on its
///   thread in place of the system's own run.
unsafe fn write_ranges(engine: &Engine, ctx: *mut u64) {
    drop_stale_buffers();
    LEVELS.with_borrow_mut(|levels| {
        // SAFETY: the caller guarantees the live context of
        // `upgrade_building_range` on its thread, in place of its run.
        unsafe { write_ranges_into(engine, ctx, levels) };
        merge(&mut Vec::new(), &mut Vec::new(), levels);
    });
}

/// Writes each row's range as [`write_ranges`] describes, into `levels` the
/// sizes it saw.
///
/// # Safety
///
/// - `ctx` is the live context of `upgrade_building_range`, used on its
///   thread in place of the system's own run.
unsafe fn write_ranges_into(engine: &Engine, ctx: *mut u64, levels: &mut Vec<(u32, Sizes)>) {
    let Some(range_at) = FIELDS.get().map(|fields| fields.range) else {
        return;
    };
    let targets = TARGETS.load();
    // SAFETY: the caller guarantees the system's live context on its thread;
    // its iterator is `UPGRADE_WORDS` words, checked against the system's
    // reflected components at link; the query ends inside this call.
    let mut query: Query<'_, UPGRADE_WORDS> = unsafe { engine.query(ctx) };
    while let Some(words) = query.next() {
        let words = *words;
        // SAFETY: `words` are the current row's iterator words of the live
        // query, on the system's thread.
        let Some(level) = (unsafe { altar_level(engine, &words) }) else {
            continue;
        };
        // SAFETY: `words` are the current row's iterator words of the live
        // query, on the system's thread.
        let Some((current, sizes)) = (unsafe { level_sizes(engine, &words, level) }) else {
            continue;
        };
        // SAFETY: the caller guarantees the system's live context on its
        // thread.
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
        // SAFETY: `shape` is the row's non-null `TriggerShape` by the check
        // before it; its half extents sit at the reflected `rangeX` field.
        let written: [f32; 3] = unsafe { read_at(shape, range_at) };
        if written.map(f32::to_bits) != target.map(f32::to_bits) {
            // SAFETY: the tracker, stamps, entity and kind are the current
            // row's words of the live query, whose system writes
            // `TriggerShape`, on its thread.
            unsafe {
                engine.mark_changed(
                    words[UPGRADE_TRACKER],
                    words[UPGRADE_STAMP_A],
                    words[UPGRADE_STAMP_B],
                    words[UPGRADE_ENTITY],
                    words[UPGRADE_KIND],
                );
            }
            // SAFETY: `shape` is the row's non-null `TriggerShape`, which
            // this system writes, on its thread in place of its run; the half
            // extents sit at the reflected `rangeX` field.
            unsafe { write_at(shape, range_at, target) };
        }
    }
}

/// The altar level of the current row, or `None` for a row that is no altar.
///
/// # Safety
///
/// - `words` are the current row's iterator words of a live
///   `upgrade_building_range` query, used on its system's thread.
unsafe fn altar_level(engine: &Engine, words: &[u64; UPGRADE_WORDS]) -> Option<usize> {
    let level_component = words[UPGRADE_LEVEL] as *const u8;
    // SAFETY: the caller guarantees the current row's words, so these are
    // the row's null or live `Level` attribute set and its attribute
    // registry, on the system's thread.
    let value = unsafe { engine.level(words[UPGRADE_ATTRIBUTES], level_component) }?;
    // The level is the attribute's low byte.
    Some(usize::from(value.to_le_bytes()[0]))
}

/// The current level's size and the sizes of every level up to it; `None`
/// when a size is missing.
///
/// # Safety
///
/// - `words` are the current row's iterator words of a live
///   `upgrade_building_range` query, used on its system's thread.
unsafe fn level_sizes(
    engine: &Engine,
    words: &[u64; UPGRADE_WORDS],
    level: usize,
) -> Option<([f32; 3], Sizes)> {
    if level > MAX_LEVELS {
        return None;
    }
    // SAFETY: the caller guarantees the current row's words, so this is the
    // live row's balancing registry word, on the system's thread.
    let size_of = |l: usize| unsafe { engine.zone_size(words[UPGRADE_BALANCING], l) };
    let mut sizes = Sizes::new();
    for l in 1..=level {
        sizes.try_push(size_of(l)?).ok()?;
    }
    let current = match sizes.last() {
        Some(&current) => current,
        None => size_of(level)?,
    };
    Some((current, sizes))
}

unsafe extern "system" fn seam(zones: *const u8, corners: *const i64, margin: f32) -> u8 {
    // SAFETY: this detour runs only as the `building.seam_check` link, so the
    // slot holds the next function of that signature; the game's arguments
    // pass unchanged.
    let vanilla = unsafe { SEAM_NEXT.get()(zones, corners, margin) };
    if vanilla != 0 || zones.is_null() || corners.is_null() {
        return vanilla;
    }
    let (Some(layout), Some(filters)) = (SEAM.get(), FILTERS.sealed()) else {
        return vanilla;
    };
    // SAFETY: `zones` and `corners` are non-null by the check before it and are
    // the seam check's own arguments, the engine's zone list and the
    // object's two corners, live for the call.
    let accepted =
        std::panic::catch_unwind(|| unsafe { check(layout, filters, zones, corners, margin) });
    u8::from(accepted.unwrap_or(false))
}

/// Asks the filters about a placement vanilla refused.
///
/// # Safety
///
/// - `list` is the engine's live zone list, with the layout `layout` names.
/// - `corners` is readable for two corner points of `layout.corner_size`
///   bytes, each starting with three `i64`.
/// - Nothing writes either during the call.
#[expect(clippy::cast_possible_truncation, reason = "margins are a few metres")]
unsafe fn check(
    layout: &SeamLayout,
    filters: &[Filter],
    list: *const u8,
    corners: *const i64,
    margin: f32,
) -> bool {
    // SAFETY: the caller guarantees the live zone list; its entries pointer
    // sits at the build table's `building.zone_list_entries` offset.
    let entries = unsafe { read_ptr_at::<u8>(list, layout.entries) }.cast_const();
    // SAFETY: the caller guarantees the live zone list; its entry count sits
    // at the build table's `building.zone_list_count` offset.
    let count: usize = unsafe { read_at(list, layout.count) };
    if entries.is_null() || count > MAX_ZONES {
        return false;
    }
    let mut zones: ArrayVec<Bounds, MAX_ZONES> = ArrayVec::new();
    for i in 0..count {
        let entry = entries.wrapping_add(i * layout.entry_size);
        // SAFETY: `entries` is non-null by the check before it and `i` below the
        // list's count, so `entry` is an entry of the build table's
        // `building.zone_entry_size`; its u8 ignored mark sits at
        // `building.zone_entry_ignored`.
        let ignored: u8 = unsafe { read_at(entry, layout.entry_ignored) };
        // SAFETY: `entry` is an entry of the live list, `i` below its count;
        // its u8 shape sits at the build table's `building.zone_entry_shape`
        // offset.
        let shape: u8 = unsafe { read_at(entry, layout.entry_shape) };
        if ignored != 0 || shape != 0 {
            continue;
        }
        // SAFETY: `entry` is an entry of the live list, `i` below its count,
        // and an entry starts with min and max as six i64.
        let bounds: [i64; 6] = unsafe { read_at(entry, 0) };
        zones.push(Bounds {
            min: [bounds[0], bounds[1], bounds[2]],
            max: [bounds[3], bounds[4], bounds[5]],
        });
    }
    // SAFETY: the caller guarantees two readable corners, the first starting
    // with three i64.
    let low: [i64; 3] = unsafe { read_at(corners.cast(), 0) };
    // SAFETY: the caller guarantees two readable corners, the second
    // `building.corner_size` bytes after the first, starting with three i64.
    let high: [i64; 3] = unsafe { read_at(corners.cast(), layout.corner_size) };
    let call = ZoneCheck {
        header: Header::of::<ZoneCheck>(),
        zones: zones.as_ptr(),
        zone_count: zones.len(),
        object: Bounds {
            min: [0, 1, 2].map(|a| low[a].min(high[a])),
            max: [0, 1, 2].map(|a| low[a].max(high[a])),
        },
        margin: (f64::from(margin) * enshwick::ecs::FIXED_PER_METRE) as i64,
    };
    filters.iter().filter(|f| f.live()).any(|f| {
        // SAFETY: `f` is live, so the mod's DLL that holds the callback stays
        // loaded; the callback gets the user pointer its mod registered and
        // `call`, which lives for the call.
        crate::profile::call(f.owner, crate::profile::Feature::Altars, || unsafe {
            (f.callback)(&raw const call, f.user()) != 0
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_held_back_from_a_gone_world_never_reach_the_next() {
        drop_stale_buffers();
        BUILT.with_borrow_mut(|built| built.push((7, [1.0; 3])));
        drop_stale_buffers();
        assert_eq!(BUILT.with_borrow(Vec::len), 1);
        forget();
        drop_stale_buffers();
        assert!(BUILT.with_borrow(Vec::is_empty));
    }

    #[test]
    fn seen_merges_batches_and_reports_changes() {
        let now = When {
            at: Instant::now(),
            step: 0,
        };
        let later = When {
            at: now.at + FORGET_AFTER,
            step: FORGET_STEPS,
        };
        let mut seen: Seen<u8> = Seen::new();
        assert!(seen.merge([(1, 10), (2, 20)], now));
        assert!(!seen.merge([(1, 10)], now));
        assert!(seen.merge([(2, 21)], now));
        assert_eq!(seen.get(2), Some(&21));
        assert_eq!(seen.iter().count(), 2);
        assert!(seen.merge([], later));
        assert_eq!(seen.iter().count(), 0);
    }

    #[test]
    fn a_stalled_server_keeps_its_altars_and_the_last_one_goes() {
        let start = When {
            at: Instant::now(),
            step: 100,
        };
        let mut seen: Seen<u8> = Seen::new();
        assert!(seen.merge([(1, 10), (2, 20)], start));
        let stalled = When {
            at: start.at + FORGET_AFTER * 30,
            step: start.step + 1,
        };
        assert!(
            !seen.merge([(1, 10)], stalled),
            "a long step forgets nothing"
        );
        assert!(!seen.expire(stalled));
        let fast = When {
            at: start.at,
            step: start.step + FORGET_STEPS * 10,
        };
        assert!(!seen.expire(fast), "fast steps alone forget nothing");
        let gone = When {
            at: stalled.at + FORGET_AFTER,
            step: stalled.step + FORGET_STEPS,
        };
        assert!(seen.expire(gone), "no pass sees them any more");
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
