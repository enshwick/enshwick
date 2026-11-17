//! The waypoints all players see, read and watched as the loader does:
//! [`HostApi::waypoint`](enshwick_core::abi::HostApi::waypoint) and
//! `Event::WAYPOINT_CHANGED`, through the base's one watcher.

use enshwick_base::__internal::waypoints::{self, WAYPOINT_EVERY_MS, Waypoints};
use enshwick_core::__fake::ids;
use enshwick_core::abi::{Event, Metres, ModHandle, Status, WaypointData};
use enshwick_core::{Entity, PlayerKey};

use super::no_world;
use crate::test::table::{State, with};

/// What the fake's watcher keeps between its looks.
#[derive(Debug, Default)]
pub(crate) struct Watched {
    waypoints: Waypoints,
    /// When it last looked, on the fake's clock.
    looked: Option<u64>,
}

/// `entity`'s waypoint as the loader reads it, through the layout the
/// fake's reflection gives `PlayerWaypoint`: [`Status::UNAVAILABLE`] when
/// it lacks the type or its fields lie past it.
fn read(s: &State, entity: Entity) -> Result<Option<Metres>, Status> {
    let base = &s.host.base;
    let hash = base.type_hashes.get(ids::PLAYER_WAYPOINT);
    let size = base
        .type_sizes
        .get(ids::PLAYER_WAYPOINT)
        .map(|&size| size as usize);
    let enabled = base.constants.get(ids::PLAYER_WAYPOINT_ENABLED);
    let position = base.constants.get(ids::PLAYER_WAYPOINT_POSITION);
    let (Some(&hash), Some(size), Some(&enabled), Some(&position)) =
        (hash, size, enabled, position)
    else {
        return Err(Status::UNAVAILABLE);
    };
    let (Ok(enabled), Ok(position)) = (usize::try_from(enabled), usize::try_from(position)) else {
        return Err(Status::UNAVAILABLE);
    };
    if enabled >= size || position.checked_add(24).is_none_or(|end| end > size) {
        return Err(Status::UNAVAILABLE);
    }
    let Some(&(address, len)) = base.components.get(&(entity.0, hash)) else {
        return Ok(None);
    };
    if len < size {
        return Err(Status::UNAVAILABLE);
    }
    let data: *const u8 = core::ptr::with_exposed_provenance(address);
    // SAFETY: `component` leaked `len` initialised bytes at `address`, at
    // least the type's `size`, and the checks above keep the byte at
    // `enabled` and the 24 bytes at `position` inside it; the fake's state
    // lock is held, so no test writes them now.
    let bytes = unsafe { core::slice::from_raw_parts(data, size) };
    let at = |axis: usize| {
        let from = position + axis * 8;
        bytes
            .get(from..from + 8)
            .and_then(|b| b.try_into().ok())
            .map(i64::from_le_bytes)
    };
    let (Some(&shown), Some(x), Some(y), Some(z)) = (bytes.get(enabled), at(0), at(1), at(2))
    else {
        return Err(Status::UNAVAILABLE);
    };
    Ok(waypoints::position(shown, [x, y, z]))
}

/// The player online on `entity`, if any.
fn key_of(s: &State, entity: Entity) -> Option<PlayerKey> {
    s.host
        .base
        .players
        .iter()
        .find(|p| p.entity == entity)
        .map(|p| p.key)
}

pub(crate) extern "system" fn waypoint(
    _me: ModHandle,
    entity: Entity,
    out: *mut WaypointData,
) -> Status {
    if !crate::test::table::on_game_thread() {
        return Status::WRONG_THREAD;
    }
    let data = with(Err(Status::MISSING), |s| {
        if no_world(s) {
            return Err(Status::UNAVAILABLE);
        }
        let key = key_of(s, entity).unwrap_or(PlayerKey::new(0));
        Ok(waypoints::data(key, entity, read(s, entity)?))
    });
    let data = match data {
        Ok(data) => data,
        Err(status) => return status,
    };
    // SAFETY: the safe layer passes a headed struct for the call.
    if unsafe { enshwick_core::abi::write_headed(out, data) } {
        Status::OK
    } else {
        Status::BAD_ARGS
    }
}

/// What the loader runs after a step: a look at the waypoints every
/// [`WAYPOINT_EVERY_MS`] while the mod hears `Event::WAYPOINT_CHANGED`,
/// and the event for each change.
pub(crate) fn after_step() {
    let changes = with(Vec::new(), |s| {
        let heard = s
            .base
            .subscriptions
            .iter()
            .any(|sub| sub.0 == Event::WAYPOINT_CHANGED && !sub.1.owner_off(&s.base.off));
        if !heard || no_world(s) {
            s.world.waypoints = Watched::default();
            return Vec::new();
        }
        let now = s.base.ticks.nanos();
        let every = WAYPOINT_EVERY_MS * 1_000_000;
        if s.world
            .waypoints
            .looked
            .is_some_and(|at| now.saturating_sub(at) < every)
        {
            return Vec::new();
        }
        let mut changes = Vec::new();
        let players = s.host.base.players.clone();
        let frame = s.host.world.map_frame;
        let mut watched = core::mem::take(&mut s.world.waypoints);
        watched.looked = Some(now);
        watched.waypoints.look(
            &players,
            frame.as_ref(),
            |entity| read(s, entity).ok(),
            |data| changes.push(*data),
        );
        s.world.waypoints = watched;
        changes
    });
    for data in &changes {
        // SAFETY: `data` lives for the call, a headed `WaypointData`.
        unsafe {
            crate::test::base::events::dispatch(Event::WAYPOINT_CHANGED, (&raw const *data).cast())
        };
    }
}
