//! Flame altar zones on the server.
//!
//! Two system hooks and a solver:
//!
//! - after `add_buildzones_server` runs, its query is walked again to learn
//!   every altar's position and orientation;
//! - `upgrade_building_range` is replaced by the same loop vanilla runs, on the
//!   same engine helpers, except that the range it writes comes from the
//!   cluster solution when there is one; it also learns every altar's level;
//! - when positions or levels change, a background thread solves the clusters.
//!   The game thread never waits for it and keeps writing the last solution,
//!   or vanilla sizes, meanwhile.
//!
//! The engine runs a system once per batch of entities, several times a tick,
//! so no single pass sees every altar; passes merge into what is known, and an
//! altar no pass has seen for a while is forgotten.
//!
//! On a client both hooks act only for the built-in server's world while it
//! hosts a game, and what a game taught them is dropped when it stops.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use arc_swap::ArcSwap;
use arrayvec::ArrayVec;
use enshwick_api::abi::Side;
use enshwick_api::ecs::{Attributes, Ecs, Entity, Worlds};
use enshwick_api::events::Shutdown;
use enshwick_api::{Load, Next, Off};
use enshwick_geometry::{Altar, Box3, clusters};

use crate::zones::{self, Placed};

type ZoneSize = unsafe extern "system" fn(out: *mut f32, balancing: u64, level: u64) -> u64;

/// Full zone sizes for levels 1 to the current one.
pub(crate) type Sizes = ArrayVec<[f32; 3], 16>;

/// How long an altar stays known after a pass last saw it.
const FORGET_AFTER: Duration = Duration::from_secs(2);
/// `add_buildzones_server`'s iterator: context, `BaseIdComponent`,
/// `CurrentTransform`, `TriggerShape`, `BuildZoneSystem`.
const ZONES_WORDS: usize = 5;
const ZONES_TRANSFORM: usize = 2;
const ZONES_SHAPE: usize = 3;
/// `CurrentTransform`: position at 0, orientation at 0x18.
const TRANSFORM_ORIENTATION: usize = 0x18;
/// `upgrade_building_range`'s iterator: context, `Level`, `TriggerShape`, two
/// stamps, entity record, type and flags, tracker, `BalancingRegistry`,
/// `AttributeRegistry`.
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
const SHAPE_RANGE: usize = 4;
/// Vanilla writes half the level's size.
const HALF: f32 = 0.5;

/// The engine helpers and layout this module uses.
#[derive(Debug)]
struct Engine {
    ecs: Ecs,
    attributes: Attributes,
    worlds: Worlds,
    zone_size: ZoneSize,
    level_attribute: u32,
}

static ENGINE: OnceLock<Engine> = OnceLock::new();
/// Set when anything goes wrong; from then on both hooks run vanilla.
static OFF: AtomicBool = AtomicBool::new(false);

/// What the passes have seen, merged across batches.
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
    fn merge(&mut self, batch: impl IntoIterator<Item = (u32, T)>) -> bool {
        let now = Instant::now();
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

type Input = BTreeMap<u32, (Placed, Sizes)>;
type Targets = BTreeMap<u32, (usize, [f32; 3])>;

/// What the passes have seen and whether the solver runs.
#[derive(Debug)]
struct State {
    placed: Seen<Placed>,
    levels: Seen<Sizes>,
    built: Seen<[f32; 3]>,
    solved: Option<Input>,
    solving: bool,
}

static STATE: Mutex<State> = Mutex::new(State {
    placed: Seen::new(),
    levels: Seen::new(),
    built: Seen::new(),
    solved: None,
    solving: false,
});

/// The range to write per altar, with the level it was solved at; swapped whole
/// by the solver, read without a lock by every pass.
static TARGETS: OnceLock<ArcSwap<Targets>> = OnceLock::new();

thread_local! {
    static PLACED_BUFFER: RefCell<Vec<(u32, Placed)>> = const { RefCell::new(Vec::new()) };
    static BUILT_BUFFER: RefCell<Vec<(u32, [f32; 3])>> = const { RefCell::new(Vec::new()) };
    static LEVELS_BUFFER: RefCell<Vec<(u32, Sizes)>> = const { RefCell::new(Vec::new()) };
}

enshwick_api::detour! {
    static ADD_ZONES: unsafe extern "system" fn(ctx: *mut u64) = add_zones;
}

enshwick_api::detour! {
    static UPGRADE: unsafe extern "system" fn(ctx: *mut u64) = upgrade,
        flags = enshwick_api::abi::HookFlags::EXCLUSIVE;
}

/// Turns a resolved address into a function pointer of type `F`.
///
/// # Safety
///
/// `address` is a function of type `F`.
unsafe fn function<F: Copy>(address: u64) -> F {
    let address = usize::try_from(address).unwrap_or(0);
    // SAFETY: forwarded from the caller; `F` is a function pointer.
    unsafe { core::mem::transmute_copy(&address) }
}

/// Links both system hooks.
pub(crate) fn link(load: &mut Load<'_>) -> Result<(), Off> {
    let zone_size = load.symbol("balancing.zone_size")?;
    let engine = Engine {
        ecs: Ecs::resolve(load)?,
        attributes: Attributes::resolve(load)?,
        worlds: Worlds::resolve(load)?,
        // SAFETY: the loader pins this id to the zone size lookup.
        zone_size: unsafe { function(zone_size) },
        level_attribute: u32::try_from(load.constant("altar.level_attribute")?)
            .map_err(|_| Off::new("altar.level_attribute out of range"))?,
    };
    let _first = ENGINE.set(engine);
    let _targets = TARGETS.set(ArcSwap::from_pointee(Targets::new()));
    load.hook(&ADD_ZONES, load.system("add_buildzones_server")?)?;
    load.hook(&UPGRADE, load.system("upgrade_building_range")?)?;
    // Entity ids belong to one game; a client may host another next. The
    // dedicated server keeps its solution through the final save.
    if load.host().build().side == Side::CLIENT {
        load.on::<Shutdown>(|()| forget())?;
    }
    Ok(())
}

/// Drops everything a game taught the passes.
fn forget() {
    if let Ok(mut state) = STATE.lock() {
        state.placed = Seen::new();
        state.levels = Seen::new();
        state.built = Seen::new();
        state.solved = None;
    }
    if let Some(targets) = TARGETS.get() {
        targets.store(Arc::new(Targets::new()));
    }
}

fn turn_off(why: &str) {
    if !OFF.swap(true, Ordering::AcqRel) {
        enshwick_api::warn!("altar zones off, vanilla from now on: {why}");
    }
}

fn add_zones(next: Next<unsafe extern "system" fn(*mut u64)>, ctx: *mut u64) {
    // SAFETY: the original system takes its own context.
    unsafe { next.get()(ctx) };
    if OFF.load(Ordering::Acquire) {
        return;
    }
    if let Some(engine) = ENGINE.get()
        // SAFETY: the system's live context.
        && unsafe { engine.worlds.is_server(ctx) }
        // SAFETY: called with the system's live context, after vanilla ran.
        && std::panic::catch_unwind(|| unsafe { observe(engine, ctx) }).is_err()
    {
        turn_off("the position pass panicked");
    }
}

fn upgrade(next: Next<unsafe extern "system" fn(*mut u64)>, ctx: *mut u64) {
    if !OFF.load(Ordering::Acquire)
        && let Some(engine) = ENGINE.get()
        // SAFETY: the system's live context.
        && unsafe { engine.worlds.is_server(ctx) }
    {
        // SAFETY: called with the system's live context.
        if std::panic::catch_unwind(|| unsafe { write_ranges(engine, ctx) }).is_ok() {
            return;
        }
        turn_off("the range pass panicked");
        // Hand the tick to vanilla from the first row.
        // SAFETY: the system's live context.
        unsafe { engine.ecs.set_row(ctx, 0) };
    }
    // SAFETY: the original system takes its own context.
    unsafe { next.get()(ctx) };
}

/// Reads a `T` at `offset` bytes past `base`.
///
/// # Safety
///
/// A readable `T` lies there.
unsafe fn read_at<T: Copy>(base: *const u8, offset: usize) -> T {
    // SAFETY: forwarded from the caller.
    unsafe { base.wrapping_add(offset).cast::<T>().read_unaligned() }
}

/// Learns positions and orientations, after vanilla built the zones.
///
/// # Safety
///
/// `ctx` is the live context of `add_buildzones_server`.
unsafe fn observe(engine: &Engine, ctx: *mut u64) {
    PLACED_BUFFER.with_borrow_mut(|placed| {
        BUILT_BUFFER.with_borrow_mut(|built| {
            placed.clear();
            built.clear();
            // Vanilla left the row counter at the end: walk again from the start
            // and restore it.
            // SAFETY: the system's live context.
            let saved = unsafe { engine.ecs.row(ctx) };
            // SAFETY: as above.
            unsafe { engine.ecs.set_row(ctx, 0) };
            // SAFETY: as above, with the system's iterator size.
            let mut query = unsafe { engine.ecs.query::<ZONES_WORDS>(ctx) };
            while let Some(words) = query.next() {
                let transform = words[ZONES_TRANSFORM] as *const u8;
                let shape = words[ZONES_SHAPE] as *const u8;
                // SAFETY: the system's live context.
                let Entity(id) = unsafe { engine.ecs.entity(ctx) };
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
            unsafe { engine.ecs.set_row(ctx, saved) };
            let Ok(mut state) = STATE.try_lock() else {
                return;
            };
            if state.built.merge(built.drain(..)) {
                let ranges: Vec<(u32, &[f32; 3])> = state.built.iter().collect();
                enshwick_api::info!("zones built from ranges {ranges:?}");
            }
            if state.placed.merge(placed.drain(..)) {
                maybe_solve(&mut state);
            }
        });
    });
}

/// Vanilla's `upgrade_building_range`, writing solved ranges where there are
/// any.
///
/// # Safety
///
/// `ctx` is the live context of `upgrade_building_range`.
unsafe fn write_ranges(engine: &Engine, ctx: *mut u64) {
    let targets = TARGETS.get().map(ArcSwap::load_full).unwrap_or_default();
    LEVELS_BUFFER.with_borrow_mut(|levels| {
        levels.clear();
        // SAFETY: the system's live context.
        let mut query = unsafe { engine.ecs.query::<UPGRADE_WORDS>(ctx) };
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
            let Entity(id) = unsafe { engine.ecs.entity(ctx) };
            let target = match targets.get(&id) {
                Some(&(solved_level, range)) if solved_level == level => range,
                _ => current.map(|v| v * HALF),
            };
            levels.push((id, sizes));
            let shape = words[UPGRADE_SHAPE] as *mut u8;
            // SAFETY: the trigger shape this system writes holds its range here.
            let written: [f32; 3] = unsafe { read_at(shape, SHAPE_RANGE) };
            if written.map(f32::to_bits) != target.map(f32::to_bits) {
                // SAFETY: the change-tracking words of the current row.
                unsafe {
                    engine.ecs.mark_changed(
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
        let Ok(mut state) = STATE.try_lock() else {
            return;
        };
        if state.levels.merge(levels.drain(..)) {
            maybe_solve(&mut state);
        }
    });
}

/// The altar level of the current row, or `None` for a row that is no altar.
///
/// # Safety
///
/// `words` are the current row's iterator words.
unsafe fn altar_level(engine: &Engine, words: &[u64; UPGRADE_WORDS]) -> Option<usize> {
    let level_component = words[UPGRADE_LEVEL] as *const u8;
    // SAFETY: the row's `Level` component and attribute registry.
    let value = unsafe {
        engine.attributes.value(
            words[UPGRADE_ATTRIBUTES],
            level_component,
            engine.level_attribute,
        )
    }?;
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
    let size_of = |l: usize| {
        let mut out = [0f32; 4];
        // SAFETY: the balancing registry of the row; `out` has room for the
        // three floats and the valid byte.
        unsafe { (engine.zone_size)(out.as_mut_ptr(), words[UPGRADE_BALANCING], l as u64) };
        // SAFETY: the valid byte at 0xc of `out`.
        let valid = unsafe { read_at::<u8>(out.as_ptr().cast(), 0xc) } != 0;
        valid.then_some([out[0], out[1], out[2]])
    };
    let current = size_of(level)?;
    let mut sizes = Sizes::new();
    for l in 1..=level {
        sizes.try_push(size_of(l)?).ok()?;
    }
    Some((current, sizes))
}

/// Starts the solver when the input changed and it is not already running.
fn maybe_solve(state: &mut State) {
    if state.solving {
        return;
    }
    let input: Input = state
        .placed
        .iter()
        .filter_map(|(id, &placed)| Some((id, (placed, state.levels.get(id)?.clone()))))
        .collect();
    if state.solved.as_ref() == Some(&input) {
        return;
    }
    let levels: Vec<(u32, usize)> = state.levels.iter().map(|(id, s)| (id, s.len())).collect();
    enshwick_api::info!(
        "{} altars placed, levelled {levels:?}",
        state.placed.entries.len()
    );
    state.solving = true;
    let spawned = std::thread::Builder::new()
        .name("altar-zones-solver".into())
        .spawn(move || {
            let solved = std::panic::catch_unwind(|| solve(&input));
            let Ok(mut state) = STATE.lock() else {
                return;
            };
            state.solving = false;
            match solved {
                Ok(targets) => {
                    if let Some(slot) = TARGETS.get() {
                        slot.store(Arc::new(targets));
                    }
                    state.solved = Some(input);
                    // The input may have moved on while this ran.
                    maybe_solve(&mut state);
                }
                Err(_) => turn_off("the solver panicked"),
            }
        });
    if spawned.is_err() {
        state.solving = false;
    }
}

/// The range each altar should have.
fn solve(input: &Input) -> Targets {
    let started = Instant::now();
    let mut altars = Vec::new();
    let mut shapes = BTreeMap::new();
    for (&id, (placed, sizes)) in input {
        let (Some(turn), Some(centre)) = (zones::turn(placed.orientation), zones::centre(placed))
        else {
            enshwick_api::warn!("altar {id} is tilted; it keeps its vanilla zone");
            continue;
        };
        let Some(last) = sizes.last() else {
            continue;
        };
        let levels = sizes
            .iter()
            .map(|s| zones::cells(centre, s.map(|v| v * HALF), turn))
            .collect();
        altars.push(Altar {
            key: u64::from(id),
            levels,
        });
        shapes.insert(id, (sizes.len(), last.map(|v| v * HALF), turn));
    }

    let placements = match clusters(&altars) {
        Ok(placements) => placements,
        Err(error) => {
            enshwick_api::warn!("cluster input rejected: {error}; altars keep vanilla zones");
            return Targets::new();
        }
    };
    let mut targets = Targets::new();
    for placement in &placements {
        let Ok(id) = u32::try_from(placement.key) else {
            continue;
        };
        let Some(&(level, range, turn)) = shapes.get(&id) else {
            continue;
        };
        let height = placement.zone.max[1] - placement.zone.min[1];
        let target = zones::grown_range(range, turn, placement.growth, height);
        enshwick_api::info!(
            "altar {id} level {level}: cluster {}, growth {} cells, range {range:?} -> {target:?}",
            placement.cluster,
            placement.growth
        );
        targets.insert(id, (level, target));
    }
    enshwick_api::info!(
        "solved {} altars in {:?}",
        placements.len(),
        started.elapsed()
    );
    targets
}

/// Every altar's zone as the engine builds it now, in whole cells, keyed by the
/// altar's entity.
#[must_use]
pub(crate) fn zones() -> Vec<(u32, Box3)> {
    let Ok(state) = STATE.lock() else {
        return Vec::new();
    };
    state
        .placed
        .iter()
        .filter_map(|(id, placed)| {
            let range = *state.built.get(id)?;
            let turn = zones::turn(placed.orientation)?;
            let centre = zones::centre(placed)?;
            Some((id, zones::cells(centre, range, turn)))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seen_merges_batches_and_reports_changes() {
        let mut seen: Seen<u8> = Seen::new();
        assert!(seen.merge([(1, 10), (2, 20)]));
        assert!(!seen.merge([(1, 10)]));
        assert!(seen.merge([(2, 21)]));
        assert_eq!(seen.get(2), Some(&21));
        assert_eq!(seen.iter().count(), 2);
    }
}
