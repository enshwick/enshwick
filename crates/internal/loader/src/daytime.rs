//! The clock of the server's world: the time of day, its speed, and the
//! turns of day and night.
//!
//! The server keeps the clock outside the ECS, in its `g38_daytime::Daytime`
//! object: an anchor (the ECS time and the game time at the last change), a
//! speed, and the time of day the `server_update_daytime` system works out
//! from them each step. Every change through the game's two setters bumps a
//! version, and the server's post-step sends the new anchor to every client
//! when the version moved, so clients jump once and follow; a raw write
//! would never reach them.
//!
//! The game's setter of the time of day restarts the game time, which the
//! weather and other schedules count in, within one day; the loader moves
//! it on to the next time the clock shows that hour instead, so a time set
//! only ever moves the world forward, as sleeping does.
//!
//! The game sets the speed itself every step from `server_nighttime_skip`:
//! 1 by day, faster at night while players sleep. Mods change it as a factor
//! on the game's own: the loader detours the speed setter and multiplies
//! what the game asks for by every mod's factor, and skips a call that would
//! change nothing, so the version moves only when the speed does.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};

use enshwick::abi::{DaytimeInfo, Event, Header, HookFlags, Status};

use crate::chains::Next;
use crate::host::{Loader, is_off, take};
use crate::locks::Relock;
use crate::mod_index::ModIndex;
use crate::pinned;
use crate::pinned::Symbol;

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[
    pinned::daytime::ANCHOR.id(),
    pinned::daytime::CLOCK.id(),
    pinned::daytime::DAY_END.id(),
    pinned::daytime::DAY_LENGTH.id(),
    pinned::daytime::DAY_START.id(),
    pinned::daytime::GAME_TIME.id(),
    pinned::daytime::IS_DAY.id(),
    pinned::daytime::NIGHT_LENGTH.id(),
    pinned::daytime::OBJECT.id(),
    pinned::daytime::SET_SPEED.id(),
    pinned::daytime::SET_TIME.id(),
    pinned::daytime::SPEED.id(),
    pinned::server::PLAYER_INFOS.id(),
    pinned::server::WORLD.id(),
    pinned::world::TIME.id(),
];

/// Game nanoseconds in a day.
const DAY: i64 = 24 * 3_600_000_000_000;
/// The largest factor a mod may set on the clock's speed.
const MAX_FACTOR: f32 = 1000.0;
/// No change of the speed smaller than this, relative, is worth a message
/// to every client: the game's own threshold.
const SPEED_EPSILON: f32 = 0.001;

type SetTimeFn = unsafe extern "system" fn(daytime: *mut u8, time: i64, now: i64);
type SetSpeedFn = unsafe extern "system" fn(daytime: *mut u8, speed: f32, now: i64);
type NowFn = unsafe extern "system" fn(world: *const u8) -> i64;

/// The game functions and offsets, from the build table.
#[derive(Debug)]
struct Layout {
    set_time: SetTimeFn,
    now: NowFn,
    world: usize,
    game: usize,
    daytime: usize,
    anchor: usize,
    game_time: usize,
    speed: usize,
    day_start: usize,
    day_end: usize,
    day_length: usize,
    night_length: usize,
    clock: usize,
    is_day: usize,
}

static LAYOUT: OnceLock<Option<Layout>> = OnceLock::new();
/// Mods that asked to change the speed, during init.
static SPEED_MODS: Mutex<Vec<ModIndex>> = Mutex::new(Vec::new());
/// Each speed mod and its factor, as `f32` bits; sealed at link.
static FACTORS: OnceLock<Box<[(ModIndex, AtomicU32)]>> = OnceLock::new();
/// The speed the game itself last asked for, as `f32` bits.
static GAME_SPEED: AtomicU32 = AtomicU32::new(0x3f80_0000);
/// Whether it was day after the last step: 0 unknown, 1 night, 2 day.
static WAS_DAY: AtomicU32 = AtomicU32::new(0);
// SAFETY: `SetSpeedFn` is the signature of the table's `daytime.set_speed`,
// and this slot is the `next` of the one link `link` makes on it, whose
// detour `speed` calls `get`; `set_speed` calls through `wired` only.
static SPEED_NEXT: Next<SetSpeedFn> = unsafe { Next::new() };

fn layout(loader: &Loader) -> Option<&'static Layout> {
    LAYOUT
        .get_or_init(|| {
            let function = |name: Symbol| {
                loader
                    .address(name)
                    .and_then(|address| usize::try_from(address).ok())
            };
            let (set_time, now) = (
                function(pinned::daytime::SET_TIME)?,
                function(pinned::world::TIME)?,
            );
            Some(Layout {
                // SAFETY: the table's `daytime.set_time` is the address of the
                // game function with the `SetTimeFn` signature on this build,
                // in the image, which stays mapped.
                set_time: unsafe { core::mem::transmute::<usize, SetTimeFn>(set_time) },
                // SAFETY: the table's `world.time` is the address of the game
                // function with the `NowFn` signature on this build, in the
                // image, which stays mapped.
                now: unsafe { core::mem::transmute::<usize, NowFn>(now) },
                world: loader.offset(pinned::server::WORLD)?,
                game: loader.offset(pinned::server::PLAYER_INFOS)?,
                daytime: loader.offset(pinned::daytime::OBJECT)?,
                anchor: loader.offset(pinned::daytime::ANCHOR)?,
                game_time: loader.offset(pinned::daytime::GAME_TIME)?,
                speed: loader.offset(pinned::daytime::SPEED)?,
                day_start: loader.offset(pinned::daytime::DAY_START)?,
                day_end: loader.offset(pinned::daytime::DAY_END)?,
                day_length: loader.offset(pinned::daytime::DAY_LENGTH)?,
                night_length: loader.offset(pinned::daytime::NIGHT_LENGTH)?,
                clock: loader.offset(pinned::daytime::CLOCK)?,
                is_day: loader.offset(pinned::daytime::IS_DAY)?,
            })
        })
        .as_ref()
}

/// The server's `Daytime` object and its world, on the game thread.
fn daytime(loader: &Loader) -> Result<(&'static Layout, usize, usize), Status> {
    let layout = layout(loader).ok_or(Status::MISSING)?;
    let server = crate::world::server()?;
    // SAFETY: `server` comes from `crate::world::server`, on the game thread
    // that alone writes it; the table's `server.world` offset holds its world
    // pointer.
    let world: usize = unsafe { crate::memory::read(server, layout.world) };
    // SAFETY: `server` comes from `crate::world::server`, on the game thread
    // that alone writes it; the table's `server.player_infos` offset holds
    // its game object pointer.
    let game: usize = unsafe { crate::memory::read(server, layout.game) };
    if world == 0 || game == 0 {
        return Err(Status::MISSING);
    }
    // SAFETY: `game` is non-zero (`game == 0` returns first), read on the game
    // thread; the table's `daytime.object` offset holds its `Daytime` pointer
    // from its creation.
    let daytime: usize = unsafe { crate::memory::read(game, layout.daytime) };
    if daytime == 0 {
        return Err(Status::MISSING);
    }
    Ok((layout, world, daytime))
}

/// The clock as it is.
pub(crate) fn info(loader: &Loader) -> Result<DaytimeInfo, Status> {
    let (layout, _world, daytime) = daytime(loader)?;
    // SAFETY: `daytime` is the live, non-zero `Daytime` from `daytime`, on
    // the game thread that alone writes it; the table's `daytime.clock`
    // offset holds its `i64` time of day.
    let time: i64 = unsafe { crate::memory::read(daytime, layout.clock) };
    // SAFETY: `daytime` is the live, non-zero `Daytime` from `daytime`, on
    // the game thread that alone writes it; the table's `daytime.day_start`
    // offset holds its `i64` start of day.
    let day_start: i64 = unsafe { crate::memory::read(daytime, layout.day_start) };
    // SAFETY: `daytime` is the live, non-zero `Daytime` from `daytime`, on
    // the game thread that alone writes it; the table's `daytime.day_end`
    // offset holds its `i64` end of day.
    let day_end: i64 = unsafe { crate::memory::read(daytime, layout.day_end) };
    // SAFETY: `daytime` is the live, non-zero `Daytime` from `daytime`, on
    // the game thread that alone writes it; the table's `daytime.day_length`
    // offset holds its `i64` day length.
    let day_length: i64 = unsafe { crate::memory::read(daytime, layout.day_length) };
    // SAFETY: `daytime` is the live, non-zero `Daytime` from `daytime`, on
    // the game thread that alone writes it; the table's
    // `daytime.night_length` offset holds its `i64` night length.
    let night_length: i64 = unsafe { crate::memory::read(daytime, layout.night_length) };
    // SAFETY: `daytime` is the live, non-zero `Daytime` from `daytime`, on
    // the game thread that alone writes it; the table's `daytime.speed`
    // offset holds its `f32` speed.
    let speed: f32 = unsafe { crate::memory::read(daytime, layout.speed) };
    // SAFETY: `daytime` is the live, non-zero `Daytime` from `daytime`, on
    // the game thread that alone writes it; the table's `daytime.is_day`
    // offset holds its day flag byte, read as an integer.
    let is_day: u8 = unsafe { crate::memory::read(daytime, layout.is_day) };
    Ok(DaytimeInfo {
        header: Header::of::<DaytimeInfo>(),
        time,
        day_start,
        day_end,
        day_length,
        night_length,
        speed,
        flags: if is_day == 0 { 0 } else { DaytimeInfo::DAY },
    })
}

/// Sets the time of day through the game's setter.
pub(crate) fn set_time(loader: &Loader, time: i64) -> Result<(), Status> {
    let (layout, world, daytime) = daytime(loader)?;
    let time = time.rem_euclid(DAY);
    // SAFETY: `now` is the table's `world.time`, called with the server's
    // live, non-zero world (checked by `daytime`) on the game thread.
    let now = unsafe { (layout.now)(core::ptr::with_exposed_provenance(world)) };
    let before = game_time(layout, daytime, now);
    // SAFETY: `set_time` is the table's `daytime.set_time`, called as the
    // game's admin command calls it: on the live, non-zero `Daytime` (checked
    // by `daytime`), on the game thread at a quiet moment (checked by
    // `crate::world::server`).
    unsafe { (layout.set_time)(core::ptr::with_exposed_provenance_mut(daytime), time, now) };
    // SAFETY: `daytime` is the live, non-zero `Daytime` from `daytime`, on
    // the game thread that alone writes it; the table's `daytime.day_length`
    // offset holds its `i64` day length.
    let day: i64 = unsafe { crate::memory::read(daytime, layout.day_length) };
    // SAFETY: `daytime` is the live, non-zero `Daytime` from `daytime`, on
    // the game thread that alone writes it; the table's
    // `daytime.night_length` offset holds its `i64` night length.
    let night: i64 = unsafe { crate::memory::read(daytime, layout.night_length) };
    let cycle = day.saturating_add(night);
    // SAFETY: `daytime` is the live, non-zero `Daytime` from `daytime`, on
    // the game thread that alone writes it; the table's `daytime.game_time`
    // offset holds the anchor's `i64` game time, which the setter just wrote.
    let within: i64 = unsafe { crate::memory::read(daytime, layout.game_time) };
    let after = forward(before, within, cycle);
    if after != within {
        // SAFETY: `daytime` is the live, non-zero `Daytime` from `daytime`,
        // on the game thread at a quiet moment, where nothing else touches
        // it; the table's `daytime.game_time` offset holds the anchor's `i64`
        // game time, which the server sends with the version the setter
        // bumped.
        unsafe { crate::memory::write(daytime, layout.game_time, after) };
    }
    Ok(())
}

/// The game time at ECS time `now`, as the game works it out.
fn game_time(layout: &Layout, daytime: usize, now: i64) -> i64 {
    // SAFETY: callers pass the live, non-zero `Daytime` from `daytime`, on
    // the game thread that alone writes it; the table's `daytime.anchor`
    // offset holds the anchor's `i64` ECS time.
    let anchor: i64 = unsafe { crate::memory::read(daytime, layout.anchor) };
    // SAFETY: callers pass the live, non-zero `Daytime` from `daytime`, on
    // the game thread that alone writes it; the table's `daytime.game_time`
    // offset holds the anchor's `i64` game time.
    let game: i64 = unsafe { crate::memory::read(daytime, layout.game_time) };
    // SAFETY: callers pass the live, non-zero `Daytime` from `daytime`, on
    // the game thread that alone writes it; the table's `daytime.speed`
    // offset holds its `f32` speed.
    let speed: f32 = unsafe { crate::memory::read(daytime, layout.speed) };
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        reason = "the game scales the elapsed time in floating point too"
    )]
    let elapsed = (now.saturating_sub(anchor) as f64 * f64::from(speed)) as i64;
    game.saturating_add(elapsed)
}

/// `within`, a game time inside the first day, moved on by whole days to
/// the first that is not before `before`.
fn forward(before: i64, within: i64, cycle: i64) -> i64 {
    if cycle <= 0 || within >= before {
        return within;
    }
    let days = (before - within).div_euclid(cycle) + i64::from((before - within) % cycle != 0);
    within.saturating_add(days.saturating_mul(cycle))
}

/// Registers mod `owner` as one that changes the speed.
pub(crate) fn register_speed(loader: &Loader, owner: ModIndex) -> Status {
    if FACTORS.get().is_some() {
        return Status::TOO_LATE;
    }
    if layout(loader).is_none() || loader.address(pinned::daytime::SET_SPEED).is_none() {
        return Status::MISSING;
    }
    let mut mods = SPEED_MODS.relock();
    if !mods.contains(&owner) {
        mods.push(owner);
    }
    Status::OK
}

/// Sets mod `owner`'s factor on the speed and applies the product at once.
pub(crate) fn set_speed(loader: &Loader, owner: ModIndex, factor: f32) -> Result<(), Status> {
    if !(0.0..=MAX_FACTOR).contains(&factor) {
        return Err(Status::BAD_ARGS);
    }
    let slot = FACTORS
        .get()
        .and_then(|factors| factors.iter().find(|(id, _)| *id == owner))
        .ok_or(Status::REFUSED)?;
    let (layout, world, daytime) = daytime(loader)?;
    let next = SPEED_NEXT.wired().ok_or(Status::MISSING)?;
    slot.1.store(factor.to_bits(), Ordering::Relaxed);
    let wanted = f32::from_bits(GAME_SPEED.load(Ordering::Relaxed)) * product();
    // SAFETY: `daytime` is the live, non-zero `Daytime` from `daytime`, on
    // the game thread that alone writes it; the table's `daytime.speed`
    // offset holds its `f32` speed.
    let current: f32 = unsafe { crate::memory::read(daytime, layout.speed) };
    if !same(current, wanted) {
        // SAFETY: `now` is the table's `world.time`, called with the
        // server's live, non-zero world (checked by `daytime`) on the game
        // thread.
        let now = unsafe { (layout.now)(core::ptr::with_exposed_provenance(world)) };
        // SAFETY: `next` is the wired rest of the `daytime.set_speed` chain,
        // past the loader's detour, called on the live, non-zero `Daytime`
        // (checked by `daytime`) on the game thread at a quiet moment
        // (checked by `crate::world::server`), as the game calls it.
        unsafe { next(core::ptr::with_exposed_provenance_mut(daytime), wanted, now) };
    }
    Ok(())
}

/// The product of the factors of every speed mod still running.
fn product() -> f32 {
    FACTORS.get().map_or(1.0, |factors| {
        factors
            .iter()
            .filter(|(owner, _)| !is_off(ModIndex::new((*owner).get())))
            .map(|(_, factor)| f32::from_bits(factor.load(Ordering::Relaxed)))
            .product()
    })
}

/// Whether two speeds differ by less than the game bothers to send.
fn same(a: f32, b: f32) -> bool {
    (a - b).abs() <= b.abs() * SPEED_EPSILON
}

/// Seals the speed mods and links the speed setter when any asked.
pub(crate) fn link(loader: &'static Loader) {
    let mods = take(&SPEED_MODS);
    let wanted = !mods.is_empty();
    let _first = FACTORS.set(
        mods.into_iter()
            .map(|owner| (owner, AtomicU32::new(1.0f32.to_bits())))
            .collect(),
    );
    if wanted {
        loader.link_feature(
            "time speed",
            loader.address(pinned::daytime::SET_SPEED),
            &SPEED_NEXT,
            speed as SetSpeedFn,
            HookFlags::NONE,
        );
    }
}

/// The switch of the day length.
static DAYTIME_FUSE: crate::fuse::Fuse = crate::fuse::Fuse::new("day length");

unsafe extern "system" fn speed(daytime: *mut u8, game: f32, now: i64) {
    GAME_SPEED.store(game.to_bits(), Ordering::Relaxed);
    let factor = DAYTIME_FUSE.run("the speed", product).unwrap_or(1.0);
    if !same(factor, 1.0) && !daytime.is_null() {
        let wanted = game * factor;
        let at = LAYOUT
            .get()
            .and_then(Option::as_ref)
            .map(|layout| layout.speed);
        if let Some(at) = at {
            // SAFETY: `daytime` is non-null (the `is_null` condition) and the
            // live `Daytime` the game passed to `daytime.set_speed`; the
            // table's `daytime.speed` offset holds its `f32` speed, which the
            // setter writes only after this read.
            let current: f32 = unsafe { enshwick::raw::read_at(daytime, at) };
            if same(current, wanted) {
                return;
            }
        }
        // SAFETY: the game's arguments to `daytime.set_speed`, with the speed
        // scaled by the mods' factors, passed on to the rest of the chain,
        // which the slot holds since the chain is wired before the target
        // jumps here.
        unsafe { SPEED_NEXT.get()(daytime, wanted, now) };
        return;
    }
    // SAFETY: the game's arguments to `daytime.set_speed`, passed on
    // unchanged to the rest of the chain, which the slot holds since the
    // chain is wired before the target jumps here.
    unsafe { SPEED_NEXT.get()(daytime, game, now) };
}

/// Raises the turn of day or night the last step brought, on the game
/// thread after it.
pub(crate) fn after_step(loader: &Loader) {
    if !crate::host::handles(Event::DAY_STARTED) && !crate::host::handles(Event::NIGHT_STARTED) {
        // A handler that arrives later starts from its first look, as at start.
        if WAS_DAY.load(Ordering::Relaxed) != 0 {
            WAS_DAY.store(0, Ordering::Relaxed);
        }
        return;
    }
    let Ok(info) = info(loader) else {
        return;
    };
    let now = if info.flags & DaytimeInfo::DAY == 0 {
        1
    } else {
        2
    };
    let before = WAS_DAY.swap(now, Ordering::Relaxed);
    if before == 0 || before == now {
        return;
    }
    let event = if now == 2 {
        Event::DAY_STARTED
    } else {
        Event::NIGHT_STARTED
    };
    loader.emit(event, None, core::ptr::null());
}

/// Forgets the last turn when a hosted game ends, so the next starts clean.
pub(crate) fn forget() {
    WAS_DAY.store(0, Ordering::Relaxed);
    GAME_SPEED.store(1.0f32.to_bits(), Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speeds_within_the_game_s_threshold_are_the_same() {
        assert!(same(60.0, 60.03));
        assert!(!same(1.0, 1.01));
        assert!(same(0.0, 0.0));
    }

    #[test]
    fn a_time_set_moves_the_game_time_forward_by_whole_days() {
        let day = 2520;
        assert_eq!(forward(10 * day + 100, 50, day), 11 * day + 50);
        assert_eq!(forward(10 * day + 50, 50, day), 10 * day + 50);
        assert_eq!(forward(10, 50, day), 50, "already ahead");
        assert_eq!(forward(10 * day, 50, 0), 50, "no day length");
    }

    #[test]
    fn without_speed_mods_the_factor_is_one() {
        assert!((product() - 1.0).abs() < f32::EPSILON);
    }
}
