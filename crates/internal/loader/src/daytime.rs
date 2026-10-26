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
use std::sync::{Mutex, OnceLock, PoisonError};

use enshwick::abi::{DaytimeInfo, Event, Header, HookFlags, Status};

use crate::chains::Next;
use crate::host::{Loader, is_off, take};

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[
    "daytime.anchor",
    "daytime.clock",
    "daytime.day_end",
    "daytime.day_length",
    "daytime.day_start",
    "daytime.game_time",
    "daytime.is_day",
    "daytime.night_length",
    "daytime.object",
    "daytime.set_speed",
    "daytime.set_time",
    "daytime.speed",
    "server.player_infos",
    "server.world",
    "world.time",
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
static SPEED_MODS: Mutex<Vec<usize>> = Mutex::new(Vec::new());
/// Each speed mod and its factor, as `f32` bits; sealed at link.
static FACTORS: OnceLock<Box<[(usize, AtomicU32)]>> = OnceLock::new();
/// The speed the game itself last asked for, as `f32` bits.
static GAME_SPEED: AtomicU32 = AtomicU32::new(0x3f80_0000);
/// Whether it was day after the last step: 0 unknown, 1 night, 2 day.
static WAS_DAY: AtomicU32 = AtomicU32::new(0);
// SAFETY: the slot joins the chain of the speed setter, typed `SetSpeedFn`.
static SPEED_NEXT: Next<SetSpeedFn> = unsafe { Next::new() };

fn layout(loader: &Loader) -> Option<&'static Layout> {
    LAYOUT
        .get_or_init(|| {
            let function = |name: &str| {
                loader
                    .address(name)
                    .and_then(|address| usize::try_from(address).ok())
            };
            let (set_time, now) = (function("daytime.set_time")?, function("world.time")?);
            Some(Layout {
                // SAFETY: the symbol is the game's time setter with this
                // signature on this build.
                set_time: unsafe { core::mem::transmute::<usize, SetTimeFn>(set_time) },
                // SAFETY: the symbol is the game's ECS time with this
                // signature on this build.
                now: unsafe { core::mem::transmute::<usize, NowFn>(now) },
                world: loader.offset("server.world")?,
                game: loader.offset("server.player_infos")?,
                daytime: loader.offset("daytime.object")?,
                anchor: loader.offset("daytime.anchor")?,
                game_time: loader.offset("daytime.game_time")?,
                speed: loader.offset("daytime.speed")?,
                day_start: loader.offset("daytime.day_start")?,
                day_end: loader.offset("daytime.day_end")?,
                day_length: loader.offset("daytime.day_length")?,
                night_length: loader.offset("daytime.night_length")?,
                clock: loader.offset("daytime.clock")?,
                is_day: loader.offset("daytime.is_day")?,
            })
        })
        .as_ref()
}

/// The server's `Daytime` object and its world, on the game thread.
fn daytime(loader: &Loader) -> Result<(&'static Layout, usize, usize), Status> {
    let layout = layout(loader).ok_or(Status::MISSING)?;
    let server = crate::world::server()?;
    // SAFETY: the server holds its world pointer and its game object.
    let world: usize = unsafe { crate::memory::read(server, layout.world) };
    // SAFETY: as above.
    let game: usize = unsafe { crate::memory::read(server, layout.game) };
    if world == 0 || game == 0 {
        return Err(Status::MISSING);
    }
    // SAFETY: the game object holds the `Daytime` pointer from its creation.
    let daytime: usize = unsafe { crate::memory::read(game, layout.daytime) };
    if daytime == 0 {
        return Err(Status::MISSING);
    }
    Ok((layout, world, daytime))
}

/// The clock as it is.
pub(crate) fn info(loader: &Loader) -> Result<DaytimeInfo, Status> {
    let (layout, _world, daytime) = daytime(loader)?;
    let read = |at: usize| -> i64 {
        // SAFETY: each field lies inside the live `Daytime`, on the game
        // thread that writes it.
        unsafe { crate::memory::read(daytime, at) }
    };
    // SAFETY: as above.
    let speed: f32 = unsafe { crate::memory::read(daytime, layout.speed) };
    // SAFETY: as above.
    let is_day: u8 = unsafe { crate::memory::read(daytime, layout.is_day) };
    Ok(DaytimeInfo {
        header: Header::of::<DaytimeInfo>(),
        time: read(layout.clock),
        day_start: read(layout.day_start),
        day_end: read(layout.day_end),
        day_length: read(layout.day_length),
        night_length: read(layout.night_length),
        speed,
        flags: if is_day == 0 { 0 } else { DaytimeInfo::DAY },
    })
}

/// Sets the time of day through the game's setter.
pub(crate) fn set_time(loader: &Loader, time: i64) -> Result<(), Status> {
    let (layout, world, daytime) = daytime(loader)?;
    let time = time.rem_euclid(DAY);
    // SAFETY: the game's ECS time of its live world, on the game thread.
    let now = unsafe { (layout.now)(core::ptr::with_exposed_provenance(world)) };
    let before = game_time(layout, daytime, now);
    // SAFETY: the game's setter on its live `Daytime`, on the game thread
    // between steps, as its admin command calls it.
    unsafe { (layout.set_time)(core::ptr::with_exposed_provenance_mut(daytime), time, now) };
    // SAFETY: the lengths of a day and a night inside the live `Daytime`.
    let day: i64 = unsafe { crate::memory::read(daytime, layout.day_length) };
    // SAFETY: as above.
    let night: i64 = unsafe { crate::memory::read(daytime, layout.night_length) };
    let cycle = day.saturating_add(night);
    // SAFETY: the anchor's game time the setter just wrote.
    let within: i64 = unsafe { crate::memory::read(daytime, layout.game_time) };
    let after = forward(before, within, cycle);
    if after != within {
        // SAFETY: the anchor's game time inside the live `Daytime`, which
        // the server sends with the version the setter bumped.
        unsafe {
            core::ptr::with_exposed_provenance_mut::<i64>(daytime + layout.game_time)
                .write_unaligned(after);
        };
    }
    Ok(())
}

/// The game time at ECS time `now`, as the game works it out.
fn game_time(layout: &Layout, daytime: usize, now: i64) -> i64 {
    // SAFETY: the anchor and the speed inside the live `Daytime`.
    let anchor: i64 = unsafe { crate::memory::read(daytime, layout.anchor) };
    // SAFETY: as above.
    let game: i64 = unsafe { crate::memory::read(daytime, layout.game_time) };
    // SAFETY: as above.
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
pub(crate) fn register_speed(loader: &Loader, owner: usize) -> Status {
    if FACTORS.get().is_some() {
        return Status::TOO_LATE;
    }
    if layout(loader).is_none() || loader.address("daytime.set_speed").is_none() {
        return Status::MISSING;
    }
    let mut mods = SPEED_MODS.lock().unwrap_or_else(PoisonError::into_inner);
    if !mods.contains(&owner) {
        mods.push(owner);
    }
    Status::OK
}

/// Sets mod `owner`'s factor on the speed and applies the product at once.
pub(crate) fn set_speed(loader: &Loader, owner: usize, factor: f32) -> Result<(), Status> {
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
    // SAFETY: the speed lies inside the live `Daytime`, on the game thread.
    let current: f32 = unsafe { crate::memory::read(daytime, layout.speed) };
    if !same(current, wanted) {
        // SAFETY: the game's ECS time of its live world, on the game thread.
        let now = unsafe { (layout.now)(core::ptr::with_exposed_provenance(world)) };
        // SAFETY: the game's speed setter, past the loader's detour, on its
        // live `Daytime` on the game thread between steps.
        unsafe { next(core::ptr::with_exposed_provenance_mut(daytime), wanted, now) };
    }
    Ok(())
}

/// The product of the factors of every speed mod still running.
fn product() -> f32 {
    FACTORS.get().map_or(1.0, |factors| {
        factors
            .iter()
            .filter(|(owner, _)| !is_off(*owner))
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
            loader.address("daytime.set_speed"),
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
            // SAFETY: the game's live `Daytime`, passed to its setter.
            let current = unsafe { daytime.wrapping_add(at).cast::<f32>().read_unaligned() };
            if same(current, wanted) {
                return;
            }
        }
        // SAFETY: forwarded with the speed changed.
        unsafe { SPEED_NEXT.get()(daytime, wanted, now) };
        return;
    }
    // SAFETY: forwarded unchanged.
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
