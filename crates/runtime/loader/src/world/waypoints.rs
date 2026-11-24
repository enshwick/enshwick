//! The waypoints all players see: the one reader of the game's
//! `PlayerWaypoint`, behind [`HostApi::waypoint`](enshwick_core::abi::HostApi::waypoint),
//! and the one watcher that raises `Event::WAYPOINT_CHANGED` and runs the
//! maps of carried windows. It looks every [`WAYPOINT_EVERY_MS`](waypoints::WAYPOINT_EVERY_MS) after the
//! step, and only while a mod hears the event or a client holds a map;
//! once neither does, it forgets what it saw, so its next look only notes.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use enshwick_base::__internal::waypoints::{self, Waypoints};
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
        let hash = catalogue::component_hash(loader, ids::PLAYER_WAYPOINT)?;
        waypoints::fields_inside(size, enabled, position).then_some(Self {
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
    /// The maps' own feature: a panic in `changed` turns off only the
    /// maps, never the event the mods hear.
    pub(crate) fuse: &'static Fuse,
}

impl Maps {
    /// Whether the maps still want changes: on, and held by a client.
    fn wanted(&self) -> bool {
        !self.fuse.is_off() && (self.held)()
    }
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
    /// Whether the client's commands want the map's frame, from a layer
    /// above: set once a mod asks for the client's reads.
    commands: AtomicBool,
    /// When it last asked for the map's frame for client commands, in the
    /// same milliseconds.
    framed: AtomicU64,
    /// The waypoint's layout, once read; `None` for good on a build that
    /// lacks it.
    at: OnceLock<Option<WaypointAt>>,
}

static WATCHER: Watcher = Watcher {
    maps: OnceLock::new(),
    waypoints: Mutex::new(Waypoints::new()),
    looked: AtomicU64::new(0),
    commands: AtomicBool::new(false),
    framed: AtomicU64::new(0),
    at: OnceLock::new(),
};

/// Keeps a hosted world's map frame read for the client's commands, which
/// refuse a waypoint or ping outside it; for good, from any thread.
pub(crate) fn want_frame() {
    WATCHER.commands.store(true, Ordering::Relaxed);
}

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
    let key = crate::engine::world::player(loader, entity).map_or(PlayerKey::new(0), |p| p.key);
    Ok(waypoints::data(key, entity, now))
}

/// Looks at the waypoints at most every [`WAYPOINT_EVERY_MS`](waypoints::WAYPOINT_EVERY_MS) while a mod
/// still on hears [`Event::WAYPOINT_CHANGED`] or a client holds a map,
/// raises the event and runs the maps for each change.
pub(crate) fn after_step(loader: &Loader) {
    let heard = loader.hears(Event::WAYPOINT_CHANGED);
    let maps = wanted_maps();
    if idle(heard, maps) {
        if WATCHER.commands.load(Ordering::Relaxed) {
            frame_for_commands(loader);
        }
        return;
    }
    // SAFETY: `GetTickCount64` takes no arguments and has no preconditions.
    let now = unsafe { GetTickCount64() }.max(1);
    if !waypoints::due(WATCHER.looked.load(Ordering::Relaxed), now) {
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
    hand_on(loader, &changes, heard, maps);
}

/// Asks for a hosted world's map frame at most every
/// [`WAYPOINT_EVERY_MS`](waypoints::WAYPOINT_EVERY_MS), so the client's
/// waypoint and ping commands refuse a position outside it whatever other
/// mods watch; the read is cached for the scene. Called once they
/// [`want_frame`], while the watcher idles.
#[cold]
fn frame_for_commands(loader: &Loader) {
    if loader.side != enshwick_core::abi::Side::CLIENT {
        return;
    }
    // SAFETY: `GetTickCount64` takes no arguments and has no preconditions.
    let now = unsafe { GetTickCount64() }.max(1);
    if !waypoints::due(WATCHER.framed.load(Ordering::Relaxed), now) {
        return;
    }
    WATCHER.framed.store(now, Ordering::Relaxed);
    if crate::engine::world::quiet_world(loader).is_ok() {
        let _asked = crate::world::scene::frame_info(loader);
    }
}

/// The maps of carried windows while they want changes.
fn wanted_maps() -> Option<&'static Maps> {
    WATCHER.maps.get().filter(|maps| maps.wanted())
}

/// Whether the watcher idles this step: no mod `heard` the event and no
/// `maps` want changes. As it starts to, it forgets what it saw, so its
/// next look only notes.
fn idle(heard: bool, maps: Option<&Maps>) -> bool {
    if heard || maps.is_some() {
        return false;
    }
    if WATCHER.looked.swap(0, Ordering::Relaxed) != 0 {
        WATCHER.waypoints.relock().forget();
    }
    true
}

/// For the benchmark of what every step pays while nothing watches the
/// waypoints: the watcher's idle path, as `after_step` takes it with no
/// mod hearing the event and no map held.
#[cfg(feature = "test")]
#[doc(hidden)]
pub fn bench_waypoint_idle_step() -> impl FnMut() -> bool {
    || {
        idle(
            crate::engine::loader::handles(Event::WAYPOINT_CHANGED),
            wanted_maps(),
        ) && !WATCHER.commands.load(Ordering::Relaxed)
    }
}

/// Raises the event for each of `changes` when `heard`, and runs `maps`
/// for it under their own fuse.
fn hand_on(loader: &Loader, changes: &[WaypointData], heard: bool, maps: Option<&Maps>) {
    for change in changes {
        if heard {
            loader.emit(Event::WAYPOINT_CHANGED, None, (&raw const *change).cast());
        }
        if let Some(maps) = maps {
            maps.fuse
                .run("waypoint run", || (maps.changed)(loader, change));
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

    #[test]
    fn a_client_asks_for_the_map_frame_once_its_commands_want_it() {
        let server = crate::start::tests::bare_loader();
        let client = Loader {
            side: enshwick_core::abi::Side::CLIENT,
            ..crate::start::tests::bare_loader()
        };
        let framed = || WATCHER.framed.load(Ordering::Relaxed);
        after_step(&client);
        assert_eq!(framed(), 0, "no mod asked");
        want_frame();
        after_step(&server);
        assert_eq!(framed(), 0, "a dedicated server runs no client commands");
        after_step(&client);
        assert_ne!(framed(), 0, "whatever other mods watch");
    }

    #[test]
    fn a_panic_in_a_map_run_turns_off_only_the_maps() {
        static MAP_RUNS: Fuse = Fuse::new("test map runs");
        let loader = crate::start::tests::bare_loader();
        let maps = Maps {
            held: || true,
            changed: |_, _| panic!("a map run that fails"),
            fuse: &MAP_RUNS,
        };
        let change = waypoints::data(PlayerKey::new(7), Entity(40), Some([1.0, 0.0, 1.0]));
        hand_on(&loader, &[change, change], false, Some(&maps));
        assert!(MAP_RUNS.is_off(), "the maps' own fuse took the panic");
        assert!(!maps.wanted(), "the watcher stops running the maps");
    }
}
