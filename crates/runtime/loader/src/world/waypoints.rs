//! The waypoints all players see: the one reader of the game's
//! `PlayerWaypoint`, behind [`HostApi::waypoint`](enshwick_core::abi::HostApi::waypoint),
//! and the one watcher that raises `Event::WAYPOINT_CHANGED` and runs the
//! maps of carried windows. It looks every [`WAYPOINT_EVERY_MS`] after the
//! step, and only while a mod hears the event or a client holds a map;
//! once neither does, it forgets what it saw, so its next look only notes.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use enshwick_base::__internal::waypoints::{self, WAYPOINT_EVERY_MS, Waypoints};
use enshwick_base::Relock;
use enshwick_core::abi::{Event, Metres, WaypointData};
use enshwick_core::{Entity, PlayerKey};
use windows_sys::Win32::System::SystemInformation::GetTickCount64;

use crate::engine::catalogue::{self, ids};
use crate::engine::fuse::Fuse;
use crate::engine::loader::Loader;
use crate::engine::world::WorldError;

/// Where a `PlayerWaypoint` keeps what the loader reads, from the running
/// game's reflection.
#[derive(Debug)]
struct WaypointAt {
    hash: u32,
    enabled: usize,
    position: usize,
}

impl WaypointAt {
    /// The layout, when the build reflects the type with both fields
    /// inside it.
    fn resolve(loader: &Loader) -> Option<Self> {
        let size = catalogue::size(loader, ids::PLAYER_WAYPOINT)?;
        let enabled = catalogue::offset(loader, ids::PLAYER_WAYPOINT_ENABLED)?;
        let position = catalogue::offset(loader, ids::PLAYER_WAYPOINT_POSITION)?;
        let inside = enabled < size && position.checked_add(24).is_some_and(|end| end <= size);
        let hash = catalogue::component_hash(loader, ids::PLAYER_WAYPOINT)?;
        inside.then_some(Self {
            hash,
            enabled,
            position,
        })
    }

    /// Where `entity`'s waypoint is, in metres, `None` while it is off or
    /// the entity has none. On the server's game thread at a quiet moment.
    fn read(&self, loader: &Loader, entity: Entity) -> Result<Option<Metres>, WorldError> {
        let data = match crate::engine::world::component(loader, entity, self.hash) {
            Ok(data) => data.cast::<u8>(),
            Err(WorldError::Absent) => return Ok(None),
            Err(why) => return Err(why),
        };
        // SAFETY: `data` is the entity's live PlayerWaypoint (`component`
        // found it), read on the game thread at a quiet moment (the caller
        // asked `quiet_world`), so no system writes it now; `resolve` checked
        // that the byte at `enabled` lies inside the type. It is read as a
        // byte, never as a bool.
        let shown = unsafe { enshwick_core::raw::read_at::<u8>(data, self.enabled) };
        // SAFETY: `data` is the entity's live PlayerWaypoint (`component`
        // found it), read on the game thread at a quiet moment (the caller
        // asked `quiet_world`), so no system writes it now; `resolve` checked
        // that the 24 bytes from `position`, the three i64 of
        // `waypointPosition` in 32.32 fixed point, lie inside the type.
        let fixed = unsafe { enshwick_core::raw::read_at::<[i64; 3]>(data, self.position) };
        Ok(waypoints::position(shown, fixed))
    }
}

/// What the maps of carried windows, a layer above, give the watcher: whether
/// a client holds one, and what a change runs on it.
#[derive(Debug)]
pub(crate) struct Maps {
    /// Whether a client holds a window with a map.
    pub(crate) held: fn() -> bool,
    /// Runs what a waypoint change asks of its player's newest map.
    pub(crate) changed: fn(&Loader, &WaypointData),
}

/// The watcher's state.
struct Watcher {
    /// The maps of carried windows, once their layer registered them.
    maps: OnceLock<Maps>,
    /// Each player's waypoint at the last look.
    waypoints: Mutex<Waypoints>,
    /// When it last looked, in milliseconds since the system started; 0
    /// while it does not look.
    looked: AtomicU64,
    /// The waypoint's layout, once read; `None` for good on a build that
    /// lacks it.
    at: OnceLock<Option<WaypointAt>>,
}

static WATCHER: Watcher = Watcher {
    maps: OnceLock::new(),
    waypoints: Mutex::new(Waypoints::new()),
    looked: AtomicU64::new(0),
    at: OnceLock::new(),
};

/// The waypoints: off on a build whose reflection lacks the
/// `PlayerWaypoint` the loader reads.
static WAYPOINTS_FUSE: Fuse = Fuse::new("waypoints");

/// The waypoint's layout, read once; `None` while `feature`, which turns
/// off with the reason in the Mods screen and status report on a build
/// without it, is off.
fn layout(loader: &Loader, feature: &'static Fuse) -> Option<&'static WaypointAt> {
    if feature.is_off() {
        return None;
    }
    let at = WATCHER.at.get_or_init(|| WaypointAt::resolve(loader));
    if at.is_none() {
        loader.fuse_off(
            feature,
            ids::PLAYER_WAYPOINT,
            "is not reflected with the fields the loader reads, so no waypoint is read or heard",
        );
    }
    at.as_ref()
}

/// The waypoint the character `entity` has set now, for
/// `HostApi::waypoint`; its key is 0 when no player online holds it.
///
/// # Errors
///
/// As `quiet_world`; [`WorldError::Unread`] on a build without the
/// waypoint.
pub(crate) fn read_now(loader: &Loader, entity: Entity) -> Result<WaypointData, WorldError> {
    crate::engine::world::quiet_world(loader)?;
    let at = layout(loader, &WAYPOINTS_FUSE).ok_or(WorldError::Unread)?;
    let now = at.read(loader, entity)?;
    let key = crate::engine::world::players(loader)?
        .iter()
        .find(|p| p.entity == entity)
        .map_or(PlayerKey::new(0), |p| p.key);
    Ok(waypoints::data(key, entity, now))
}

/// Looks at the waypoints at most every [`WAYPOINT_EVERY_MS`] while a mod
/// hears [`Event::WAYPOINT_CHANGED`] or a client holds a map, raises the
/// event and runs the maps for each change.
pub(crate) fn after_step(loader: &Loader) {
    let heard = crate::engine::loader::handles(Event::WAYPOINT_CHANGED);
    let maps = WATCHER.maps.get().filter(|maps| (maps.held)());
    if !heard && maps.is_none() {
        if WATCHER.looked.swap(0, Ordering::Relaxed) != 0 {
            WATCHER.waypoints.relock().forget();
        }
        return;
    }
    // SAFETY: `GetTickCount64` takes no arguments and has no preconditions.
    let now = unsafe { GetTickCount64() }.max(1);
    if now.saturating_sub(WATCHER.looked.load(Ordering::Relaxed)) < WAYPOINT_EVERY_MS {
        return;
    }
    WATCHER.looked.store(now, Ordering::Relaxed);
    let Some(at) = layout(loader, &WAYPOINTS_FUSE) else {
        return;
    };
    if crate::engine::world::quiet_world(loader).is_err() {
        return;
    }
    let players = crate::engine::world::players(loader).unwrap_or_default();
    let frame = crate::world::scene::frame_info(loader).ok();
    let mut changes = Vec::new();
    let outside = WATCHER.waypoints.relock().look(
        &players,
        frame.as_ref(),
        |entity| at.read(loader, entity).ok(),
        |data| changes.push(*data),
    );
    if let Some(outside) = outside {
        crate::log!(
            WARN,
            "a waypoint outside the map is not handed on: {outside:?}"
        );
    }
    for change in &changes {
        if heard {
            loader.emit(Event::WAYPOINT_CHANGED, None, (&raw const *change).cast());
        }
        if let Some(maps) = maps {
            (maps.changed)(loader, change);
        }
    }
}

/// Has the watcher run `maps` too, from the layer that holds them.
pub(crate) fn watch_for(maps: Maps) {
    // A second registration keeps the first: one layer holds the maps.
    let _first = WATCHER.maps.set(maps);
}

/// Forgets the waypoints seen, as a hosted game ends.
pub(crate) fn forget() {
    WATCHER.looked.store(0, Ordering::Relaxed);
    WATCHER.waypoints.relock().forget();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waypoints_a_build_cannot_read_are_listed_off_not_only_logged() {
        static WAYPOINTS_OFF: Fuse = Fuse::new("test waypoints");
        let loader = crate::start::tests::bare_loader();
        assert!(layout(&loader, &WAYPOINTS_OFF).is_none());
        assert!(WAYPOINTS_OFF.is_off());
        let listed = crate::engine::diag::parts_off();
        assert!(
            listed
                .iter()
                .any(|p| p.part == WAYPOINTS_OFF.feature()
                    && p.reason.starts_with(ids::PLAYER_WAYPOINT)),
            "the reason names what the build lacks: {listed:?}"
        );
    }
}
