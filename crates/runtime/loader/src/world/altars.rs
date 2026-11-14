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

use crate::engine::mod_index::ModIndex;
use crate::engine::pinned;
use crate::engine::pinned::Constant;
use crate::engine::registry::Registry;
use crate::world::openers::Openers;
use enshwick_base::Relock;

mod geometry;
mod levels;

use core::ffi::c_void;
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use arc_swap::ArcSwapOption;
use arrayvec::ArrayVec;
use enshwick_core::abi::{
    AltarInfo, AltarZone, AltarZoneFn, Bounds, BuildingZoneCheck, BuildingZoneCheckFn, Event,
    Header, HookFlags, Side, Status,
};
use enshwick_core::altars::MAX_ZONES;
use enshwick_core::raw::{read_at, read_ptr_at};

use self::geometry::Placed;
use crate::engine::ecs::Engine;
use crate::engine::fuse::Fuse;
use crate::engine::loader::{Loader, Registered, Target};
use crate::engine::undo::Undo;

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

/// What the passes saw of every altar, merged.
#[derive(Debug)]
struct Records {
    placed: Seen<Placed>,
    levels: Seen<Sizes>,
    built: Seen<[f32; 3]>,
}

impl Records {
    const fn new() -> Self {
        Self {
            placed: Seen::new(),
            levels: Seen::new(),
            built: Seen::new(),
        }
    }
}

/// What the altars keep while the game runs.
#[derive(Debug)]
struct State {
    /// The mods that opened the altars.
    openers: Openers,
    records: Mutex<Records>,
    sizing: Sizing,
    /// The zones the filters asked for, by altar, for the range pass.
    targets: ArcSwapOption<Targets>,
    /// Set when the records changed since the last step.
    changed: AtomicBool,
    /// The server steps the post-step counted; never reset, since only the
    /// age between two steps counts and a merge racing a reset would stamp
    /// an entry with the old step.
    step: AtomicU64,
    /// Bumped when the world goes, so threads drop what they held back from
    /// it; never reset, so no thread mistakes a later world for its own.
    world: AtomicU64,
}

impl State {
    /// Forgets the world that went: its records, targets and sizing.
    fn forget(&self) {
        self.world.fetch_add(1, Ordering::AcqRel);
        *self.records.relock() = Records::new();
        self.targets.store(None);
        self.sizing.asked.store(false, Ordering::Release);
        self.sizing.running.store(0, Ordering::Release);
        self.changed.store(true, Ordering::Release);
    }

    const fn new() -> Self {
        Self {
            openers: Openers::new(),
            records: Mutex::new(Records::new()),
            sizing: Sizing {
                asked: AtomicBool::new(false),
                running: AtomicUsize::new(0),
            },
            targets: ArcSwapOption::const_empty(),
            changed: AtomicBool::new(false),
            step: AtomicU64::new(0),
            world: AtomicU64::new(0),
        }
    }
}

static STATE: State = State::new();

type Targets = BTreeMap<u32, (usize, [f32; 3])>;

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
    fn read(loader: &Loader) -> Result<Self, LinkAltarsError> {
        let field = |ty: &'static str, name: &'static str| {
            loader
                .reflection
                .field_offset(ty, name)
                .ok()
                .and_then(|v| usize::try_from(v).ok())
                .ok_or(LinkAltarsError::NoField { ty, name })
        };
        let transform = field("keen::ecs::CurrentTransform", "transform")?;
        Ok(Self {
            range: field("keen::ecs::TriggerShape", "rangeX")?,
            position: transform + field("keen::WorldTransform", "position")?,
            orientation: transform + field("keen::WorldTransform", "orientation")?,
        })
    }
}

/// Why the altars stay off on this build.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
enum LinkAltarsError {
    /// The build's reflection lacks a field the altars read.
    #[error("no {ty}.{name} in this build")]
    NoField {
        ty: &'static str,
        name: &'static str,
    },
    /// A system the altars hook no longer fits its layout.
    #[error(transparent)]
    Components(#[from] crate::engine::ecs::MatchComponentsError),
}

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

/// The altars' switch: both system links arm together or not at all, and a
/// panic in either pass turns both off.
static ALTARS: Fuse = Fuse::new("altars");
/// The switch of the range pass's writes, off once a live trigger shape
/// does not hold its found field; the other altar passes stay on. It links
/// nothing of its own.
static RANGES_FUSE: Fuse = Fuse::new("altar ranges");
/// The switch of the mods' building zone filters, which need neither altar
/// system.
static ZONE_CHECKS: Fuse = Fuse::new("zone checks");
/// The reflected system the position pass follows.
const ZONES_SYSTEM: &str = "add_buildzones_server";
/// The reflected system the range and level passes follow.
const UPGRADE_SYSTEM: &str = "upgrade_building_range";

type Filter = Registered<BuildingZoneCheckFn>;
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
    let world = STATE.world.load(Ordering::Acquire);
    if BUFFERED_WORLD.replace(world) != world {
        PLACED.with_borrow_mut(Vec::clear);
        BUILT.with_borrow_mut(Vec::clear);
        LEVELS.with_borrow_mut(Vec::clear);
    }
}

/// Opens altars for mod `owner`; during init only.
pub(crate) fn open(loader: &Loader, owner: ModIndex) -> Status {
    if loader.delivery.get().is_some() {
        return Status::TOO_LATE;
    }
    if crate::engine::ecs::engine(loader).is_none() {
        return Status::MISSING;
    }
    STATE
        .openers
        .open(owner)
        .map_or(Status::TOO_LATE, |()| Status::OK)
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
    if !SIZERS.any(|sizer| sizer.owner == owner) {
        return Status::REFUSED;
    }
    STATE.sizing.asked.store(true, Ordering::Release);
    Status::OK
}

pub(crate) fn opened(owner: ModIndex) -> bool {
    STATE.openers.opened(owner)
}

/// The filters of mods that run.
fn running_sizers() -> impl Iterator<Item = &'static Sizer> {
    SIZERS
        .sealed()
        .unwrap_or_default()
        .iter()
        .filter(|s| !crate::engine::loader::is_off(s.owner))
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
    filters: impl Iterator<Item = (ModIndex, AltarZoneFn, *mut c_void)>,
) -> Option<(i64, i64)> {
    let mut zone = vanilla;
    for (owner, callback, user) in filters {
        let before = zone;
        // SAFETY: the mod's filter with its own pointer and a zone valid for
        // the call; the safe layer catches its panics.
        crate::engine::profile::call(owner, crate::engine::profile::Feature::Altars, || unsafe {
            callback(&raw mut zone, user);
        });
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
    STATE.sizing.running.store(running.len(), Ordering::Release);
    size_zones_with(&running);
}

/// Asks `sizers` for every altar's zone and keeps what differs from vanilla
/// for the next range pass. The filters run without the state's lock, so
/// they may read the altars.
fn size_zones_with(sizers: &[&Sizer]) {
    if sizers.is_empty() {
        STATE.targets.store(None);
        return;
    }
    let vanilla: Vec<Vanilla> = {
        let state = STATE.records.relock();
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
                        altar: enshwick_core::Entity(id),
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
        let filters = sizers.iter().map(|s| (s.owner, s.callback, s.user()));
        if let Some((grow, height)) = chain(altar.zone, filters) {
            targets.insert(
                altar.zone.altar.0,
                (
                    altar.levels,
                    geometry::grown_range(altar.range, altar.turn, grow, height),
                ),
            );
        }
    }
    STATE.targets.store(Some(Arc::new(targets)));
}

pub(crate) fn list() -> Vec<AltarInfo> {
    let state = STATE.records.relock();
    state
        .placed
        .iter()
        .map(|(id, placed)| info(id, placed, state.built.get(id), state.levels.get(id)))
        .collect()
}

fn info(id: u32, placed: &Placed, built: Option<&[f32; 3]>, levels: Option<&Sizes>) -> AltarInfo {
    let mut altar = AltarInfo::EMPTY;
    altar.entity = enshwick_core::Entity(id);
    altar.position = enshwick_core::ecs::metres(placed.position);
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
        step: STATE.step.load(Ordering::Relaxed),
    }
}

pub(crate) fn after_step(loader: &Loader, now: Instant) {
    let step = STATE.step.fetch_add(1, Ordering::Relaxed) + 1;
    // Only the post-step forgets: passes run many times a step, and none
    // runs once the last altar is gone.
    if step.is_multiple_of(EXPIRE_EVERY)
        && let Some(mut state) = STATE.records.try_relock()
    {
        let now = When { at: now, step };
        let state = &mut *state;
        let gone = state.placed.expire(now) | state.built.expire(now) | state.levels.expire(now);
        if gone {
            STATE.changed.store(true, Ordering::Release);
        }
    }
    let changed = STATE.changed.swap(false, Ordering::AcqRel);
    let gone = STATE.sizing.running.load(Ordering::Acquire) != running_sizers().count();
    if changed | STATE.sizing.asked.swap(false, Ordering::AcqRel) | gone {
        size_zones();
    }
    if changed {
        loader.emit(Event::ALTARS_CHANGED, None, core::ptr::null());
    }
}

pub(crate) fn forget() {
    STATE.forget();
}

pub(crate) fn register(
    owner: ModIndex,
    callback: BuildingZoneCheckFn,
    user: *mut c_void,
) -> Status {
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
    if STATE.openers.seal().is_empty() {
        return;
    }
    if crate::engine::ecs::engine(loader).is_none() {
        let _off = loader.fuse_off(&ALTARS, "ecs helpers", crate::engine::builds::ABSENT);
        return;
    }
    let checked = Fields::read(loader).and_then(|fields| {
        crate::engine::ecs::components_match(loader, ZONES_SYSTEM, &ZONES_READS, &[])?;
        crate::engine::ecs::components_match(
            loader,
            UPGRADE_SYSTEM,
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
            let _off = loader.fuse_off(&ALTARS, "reflection", &why.to_string());
            return;
        }
    }
    let upgrade_flags = if controlled {
        HookFlags::EXCLUSIVE
    } else {
        HookFlags::NONE
    };
    let entry = |system| {
        loader
            .reflection
            .system(system)
            .ok()
            .map(enshwick_gamedata::reflect::System::entry)
    };
    let entries = [entry(ZONES_SYSTEM), entry(UPGRADE_SYSTEM)];
    if link_systems(loader, entries, upgrade_flags) {
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
}

/// Links both altar systems at their reflected `entries`, all or none, under
/// [`ALTARS`]; true when both are linked.
fn link_systems(loader: &Loader, entries: [Option<u64>; 2], upgrade_flags: HookFlags) -> bool {
    let [zones, upgrade_at] = entries;
    loader.link_fused(
        &ALTARS,
        Target::Found(ZONES_SYSTEM, zones),
        &ADD_ZONES_NEXT,
        add_zones,
        HookFlags::NONE,
    ) && loader.link_fused(
        &ALTARS,
        Target::Found(UPGRADE_SYSTEM, upgrade_at),
        &UPGRADE_NEXT,
        upgrade,
        upgrade_flags,
    )
}

fn link_seam(loader: &'static Loader) {
    let filters = FILTERS.seal();
    let wanted = !filters.is_empty();
    if !wanted {
        return;
    }
    let size = |name: Constant| loader.offset(name).ok_or(name);
    let layout = (|| {
        Ok::<_, Constant>(SeamLayout {
            entries: size(pinned::building::ZONE_LIST_ENTRIES)?,
            count: size(pinned::building::ZONE_LIST_COUNT)?,
            entry_size: size(pinned::building::ZONE_ENTRY_SIZE)?,
            entry_shape: size(pinned::building::ZONE_ENTRY_SHAPE)?,
            entry_ignored: size(pinned::building::ZONE_ENTRY_IGNORED)?,
            corner_size: size(pinned::building::CORNER_SIZE)?,
        })
    })();
    let layout = match layout {
        Ok(layout) => layout,
        Err(missing) => {
            let _off = loader.fuse_off(&ZONE_CHECKS, missing.id(), crate::engine::builds::ABSENT);
            return;
        }
    };
    let _first = SEAM.set(layout);
    let target = Target::Pinned(pinned::building::SEAM_CHECK);
    if loader.link_fused(&ZONE_CHECKS, target, &SEAM_NEXT, seam, HookFlags::NONE) {
        crate::log!(INFO, "zone checks on");
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
    unsafe { engine.is_world(ctx, crate::engine::world::server_world()) }
}

crate::engine::chains::chain_detour! {
    /// The reflected `add_buildzones_server` system entry: vanilla builds the
    /// zones, then the position pass learns where they are.
    fn add_zones(ctx: *mut u64) -> ();
    next ADD_ZONES_NEXT;
    fuse &ALTARS, "the position pass";
    |onward| {
        onward.call();
        let (Some(loader), Some(engine)) = (
            crate::engine::loader::loader(),
            crate::engine::ecs::resolved(),
        ) else {
            return;
        };
        // SAFETY: `ctx` is the hooked system's own live context, on its
        // thread.
        if unsafe { server_world(loader, engine, ctx) } {
            // SAFETY: `ctx` is the live context of `add_buildzones_server`,
            // on its thread after vanilla ran.
            unsafe { observe(engine, ctx) };
        }
    }
}

crate::engine::chains::chain_detour! {
    /// The reflected `upgrade_building_range` system entry, one [`tick`] of
    /// it on the server's world.
    fn upgrade(ctx: *mut u64) -> ();
    next UPGRADE_NEXT;
    fuse &ALTARS, "the range and level passes";
    |onward| {
        let ready = crate::engine::loader::loader()
            .zip(crate::engine::ecs::resolved())
            // SAFETY: `ctx` is the hooked system's own live context, on its
            // thread.
            .filter(|(loader, engine)| unsafe { server_world(loader, engine, ctx) });
        let Some((loader, engine)) = ready else {
            return onward.call();
        };
        let write = || {
            // SAFETY: `ctx` is the hooked system's live context, on its
            // thread until the rewind drops at the end of this pass.
            let rows = unsafe { rewind(engine, ctx) };
            // SAFETY: `ctx` is the live context of `upgrade_building_range`,
            // on its thread, and vanilla has not run this tick.
            let wrote = unsafe { write_ranges(engine, ctx, &loader.build.name) };
            if wrote {
                rows.keep();
            }
            wrote
        };
        // SAFETY: `ctx` is the live context of `upgrade_building_range`, on
        // its thread after vanilla ran.
        let read = || unsafe { read_levels(engine, ctx) };
        let sized = !RANGES_FUSE.is_off() && running_sizers().next().is_some();
        tick(sized, write, || onward.call(), read);
    }
}

/// One tick of `upgrade_building_range`: when the zones are `sized`, the
/// range pass `write`s in place of vanilla; when it writes nothing, or
/// unsized, `vanilla` runs and the levels are `read` after it.
fn tick(sized: bool, write: impl FnOnce() -> bool, vanilla: impl FnOnce(), read: impl FnOnce()) {
    if sized && write() {
        return;
    }
    vanilla();
    read();
}

/// Puts a system context's row counter back where it was when dropped,
/// unless kept: vanilla runs the tick from its start after a range pass
/// that wrote nothing or panicked part way, and after a walk of our own.
///
/// # Safety
///
/// - `ctx` is a live system context, used on its system's thread, which
///   alone writes it, until the guard drops.
unsafe fn rewind(engine: &Engine, ctx: *mut u64) -> Undo<impl FnOnce() + '_> {
    // SAFETY: the caller guarantees the live context on its thread.
    let row = unsafe { engine.row(ctx) };
    // SAFETY: the caller guarantees the live context on its thread, which
    // alone writes it, until the guard drops.
    Undo::new(move || unsafe { engine.set_row(ctx, row) })
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
    let mut state = match STATE.records.try_relock() {
        Some(state) => state,
        None if placed.len() + built.len() + levels.len() < HELD_BACK => return,
        None => STATE.records.relock(),
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
        STATE.changed.store(true, Ordering::Release);
    }
}

crate::engine::chains::chain_detour! {
    /// The table's `building.seam_check`: vanilla's answer, or the mods'
    /// filters accepting a placement vanilla refused.
    fn seam(zones: *const u8, corners: *const i64, margin: f32) -> u8;
    next SEAM_NEXT;
    fuse &ZONE_CHECKS, "the zone check";
    |onward| {
        let vanilla = onward.call();
        if vanilla != 0 || zones.is_null() || corners.is_null() {
            return vanilla;
        }
        let (Some(layout), Some(filters)) = (SEAM.get(), FILTERS.sealed()) else {
            return vanilla;
        };
        // SAFETY: `zones` and `corners` are non-null by the check before it
        // and are the seam check's own arguments, the engine's zone list and
        // the object's two corners, live for the call.
        u8::from(unsafe { check(layout, filters, zones, corners, margin) })
    }
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
    let call = BuildingZoneCheck {
        header: Header::of::<BuildingZoneCheck>(),
        zones: zones.as_ptr(),
        zone_count: zones.len(),
        object: Bounds {
            min: [0, 1, 2].map(|a| low[a].min(high[a])),
            max: [0, 1, 2].map(|a| low[a].max(high[a])),
        },
        margin: (f64::from(margin) * enshwick_core::ecs::FIXED_PER_METRE) as i64,
    };
    filters.iter().filter(|f| f.live()).any(|f| {
        // SAFETY: `f` is live, so the mod's DLL that holds the callback stays
        // loaded; the callback gets the user pointer its mod registered and
        // `call`, which lives for the call.
        crate::engine::profile::call(
            f.owner,
            crate::engine::profile::Feature::Altars,
            || unsafe { (f.callback)(&raw const call, f.user()) != 0 },
        )
    })
}

#[cfg(test)]
mod tests;
