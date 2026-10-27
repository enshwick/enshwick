//! The Flame Altars of the server's world, for the mods that opened them.
//!
//! The engine runs a system once per batch of entities, several times a
//! tick, so no pass sees every altar: passes merge into what is known, and an
//! altar no pass has seen for [`FORGET_AFTER`] and [`FORGET_STEPS`] server
//! steps is forgotten, the last one too. Counting steps keeps a server that
//! stalls from forgetting altars it could not step over. With altar zone
//! filters, `upgrade_building_range` is replaced by vanilla's own loop on the
//! same engine helpers, writing the sizes the filters chained to, in load
//! order, each on the size the last one left.

use levels::{observe, read_levels, write_ranges};

use crate::locks::Relock;
use crate::mod_index::ModIndex;
use crate::pinned;
use crate::pinned::Constant;
use crate::registry::Registry;

mod geometry;
mod levels;

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
    AltarInfo, AltarZone, AltarZoneFn, Bounds, Event, Header, HookFlags, Side, Status, ZoneCheck,
    ZoneCheckFn,
};
use enshwick::altars::MAX_ZONES;
use enshwick::raw::{read_at, read_ptr_at};

use self::geometry::Placed;
use crate::chains::Next;
use crate::ecs::Engine;
use crate::host::{Loader, Registered};

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[
    pinned::building::CORNER_SIZE.id(),
    pinned::building::SEAM_CHECK.id(),
    pinned::building::ZONE_ENTRY_IGNORED.id(),
    pinned::building::ZONE_ENTRY_SHAPE.id(),
    pinned::building::ZONE_ENTRY_SIZE.id(),
    pinned::building::ZONE_LIST_COUNT.id(),
    pinned::building::ZONE_LIST_ENTRIES.id(),
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

    /// Records one batch; true when the known values changed. Forgetting is
    /// left to the post-step's [`Seen::expire`], since passes run many times
    /// a step.
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
        changed
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
static OPENERS: Mutex<Vec<ModIndex>> = Mutex::new(Vec::new());
/// The mods' altar zone filters, in load order.
static SIZERS: Registry<Sizer> = Registry::new();
/// When the zones need sizing again.
#[derive(Debug)]
struct Sizing {
    /// Set when a mod asked for the zones to be sized again.
    asked: AtomicBool,
    /// How many filters ran at the last sizing, so a filter whose mod goes
    /// off takes its share back at the next step.
    running: AtomicUsize,
}

static SIZING: Sizing = Sizing {
    asked: AtomicBool::new(false),
    running: AtomicUsize::new(0),
};
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
type Sizer = Registered<AltarZoneFn>;

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
pub(crate) fn open(loader: &Loader, owner: ModIndex) -> Status {
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

/// Adds mod `owner`'s altar zone filter; it opens altars too. During init
/// only.
pub(crate) fn add_sizer(
    loader: &Loader,
    owner: ModIndex,
    callback: AltarZoneFn,
    user: *mut c_void,
) -> Status {
    let status = open(loader, owner);
    if !status.is_ok() {
        return status;
    }
    SIZERS
        .add(Registered::new(owner, callback, user))
        .map_or(Status::TOO_LATE, |()| Status::OK)
}

/// Has the zones sized again on the server's next step, for mod `owner`,
/// which must have a filter.
pub(crate) fn resize(owner: ModIndex) -> Status {
    let has_filter = |sizers: &[Sizer]| sizers.iter().any(|s| s.owner == owner);
    let has_filter = match SIZERS.sealed() {
        Some(sizers) => has_filter(sizers),
        // Sealed meanwhile: the sealed list has it.
        None => SIZERS
            .edit(|sizers| has_filter(sizers))
            .unwrap_or_else(|_| SIZERS.sealed().is_some_and(has_filter)),
    };
    if !has_filter {
        return Status::REFUSED;
    }
    SIZING.asked.store(true, Ordering::Release);
    Status::OK
}

pub(crate) fn opened(owner: ModIndex) -> bool {
    OPENERS.relock().contains(&owner)
}

/// The filters of mods that run.
fn running_sizers() -> impl Iterator<Item = &'static Sizer> {
    SIZERS
        .sealed()
        .unwrap_or_default()
        .iter()
        .filter(|s| !crate::host::is_off(s.owner))
}

/// One altar's vanilla zone, taken from the state before the filters run.
struct Vanilla {
    zone: AltarZone,
    levels: usize,
    range: [f32; 3],
    turn: (f64, f64),
}

/// Runs `filters` in order on `zone`, each on what the last one left: a
/// filter keeps the altar and level, cannot shrink the zone below vanilla
/// width, and a height below one cell keeps the height before it. The
/// result, or `None` when it is the vanilla zone.
fn chain(
    vanilla: AltarZone,
    filters: impl Iterator<Item = (AltarZoneFn, *mut c_void)>,
) -> Option<(i64, i64)> {
    let mut zone = vanilla;
    for (callback, user) in filters {
        let before = zone;
        // SAFETY: the mod's filter with its own pointer and a zone valid for
        // the call; the safe layer catches its panics.
        unsafe { callback(&raw mut zone, user) };
        zone.header = before.header;
        zone.altar = before.altar;
        zone.level = before.level;
        zone.reserved = before.reserved;
        zone.grow = zone.grow.max(0);
        if zone.height <= 0 {
            zone.height = before.height;
        }
    }
    ((zone.grow, zone.height) != (vanilla.grow, vanilla.height)).then_some((zone.grow, zone.height))
}

/// Asks the filters of the mods that run for every altar's zone; see
/// [`size_zones_with`].
fn size_zones() {
    let running: Vec<&Sizer> = running_sizers().collect();
    SIZING.running.store(running.len(), Ordering::Release);
    size_zones_with(&running);
}

/// Asks `sizers` for every altar's zone and keeps what differs from vanilla
/// for the next range pass. The filters run without the state's lock, so
/// they may read the altars.
fn size_zones_with(sizers: &[&Sizer]) {
    if sizers.is_empty() {
        TARGETS.store(None);
        return;
    }
    let vanilla: Vec<Vanilla> = {
        let state = STATE.relock();
        state
            .placed
            .iter()
            .filter_map(|(id, placed)| {
                let levels = state.levels.get(id)?;
                let turn = geometry::turn(placed.orientation)?;
                let centre = geometry::centre(placed)?;
                let range = levels.last()?.map(|v| v * HALF);
                let cells = geometry::cells(centre, range, turn);
                Some(Vanilla {
                    zone: AltarZone {
                        header: Header::of::<AltarZone>(),
                        altar: id,
                        level: u8::try_from(levels.len()).unwrap_or(u8::MAX),
                        reserved: [0; 3],
                        grow: 0,
                        height: cells.max[1] - cells.min[1],
                    },
                    levels: levels.len(),
                    range,
                    turn,
                })
            })
            .collect()
    };
    let mut targets = Targets::new();
    for altar in vanilla {
        let filters = sizers.iter().map(|s| (s.callback, s.user()));
        if let Some((grow, height)) = chain(altar.zone, filters) {
            targets.insert(
                altar.zone.altar,
                (
                    altar.levels,
                    geometry::grown_range(altar.range, altar.turn, grow, height),
                ),
            );
        }
    }
    TARGETS.store(Some(Arc::new(targets)));
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
    // Only the post-step forgets: passes run many times a step, and none
    // runs once the last altar is gone.
    if step.is_multiple_of(EXPIRE_EVERY)
        && let Some(mut state) = STATE.try_relock()
    {
        let now = now();
        let state = &mut *state;
        let gone = state.placed.expire(now) | state.built.expire(now) | state.levels.expire(now);
        if gone {
            CHANGED.store(true, Ordering::Release);
        }
    }
    let changed = CHANGED.swap(false, Ordering::AcqRel);
    let gone = SIZING.running.load(Ordering::Acquire) != running_sizers().count();
    if changed | SIZING.asked.swap(false, Ordering::AcqRel) | gone {
        size_zones();
    }
    if changed {
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

pub(crate) fn register(owner: ModIndex, callback: ZoneCheckFn, user: *mut c_void) -> Status {
    if FILTERS.is_sealed() {
        return Status::TOO_LATE;
    }
    FILTERS
        .add(Registered::new(owner, callback, user))
        .map_or(Status::TOO_LATE, |()| Status::OK)
}

pub(crate) fn link(loader: &'static Loader) {
    let controlled = !SIZERS.seal().is_empty();
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
    let size = |name: Constant| loader.offset(name);
    let layout = (|| {
        Some(SeamLayout {
            entries: size(pinned::building::ZONE_LIST_ENTRIES)?,
            count: size(pinned::building::ZONE_LIST_COUNT)?,
            entry_size: size(pinned::building::ZONE_ENTRY_SIZE)?,
            entry_shape: size(pinned::building::ZONE_ENTRY_SHAPE)?,
            entry_ignored: size(pinned::building::ZONE_ENTRY_IGNORED)?,
            corner_size: size(pinned::building::CORNER_SIZE)?,
        })
    })();
    let (Some(layout), Some(target)) = (layout, loader.address(pinned::building::SEAM_CHECK))
    else {
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
    if running_sizers().next().is_some() {
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
mod tests;
