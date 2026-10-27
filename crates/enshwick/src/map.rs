//! The world's map as the server knows it: its markers and each player's
//! waypoint.
//!
//! A marker is a `MapMarker` the world comes with: a town, a ruin, a
//! dungeon, a Flame shrine, a quest area, as the scene's own data places
//! it. Its type is a key into the game's marker type registry, an open set
//! the game's data defines. What a player has uncovered of the map lives
//! only on that player's machine.
//!
//! # Examples
//!
//! ```
//! use core::ops::ControlFlow;
//! use std::time::Duration;
//!
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     let map = load.map()?;
//!     let world = load.world()?;
//!     load.scheduler()?.every(Duration::from_secs(10), move || {
//!         for player in world.players() {
//!             if let Ok(Some(at)) = map.waypoint(player.entity) {
//!                 enshwick::info!("{} marks {at:?}", player.name);
//!             }
//!         }
//!         ControlFlow::Continue(())
//!     });
//!     Ok(())
//! }
//! ```

mod fog;
mod snapshot;

pub use fog::{FogError, FogImage, FogOfWar, FogState};

pub use snapshot::{
    MAP_SNAPSHOT_FORMAT, MapAltar, MapPlayer, MapPoint, MapSnapshot, SnapshotError,
};

use crate::Host;
use crate::abi::{MapMarkerInfo, Status};
use crate::altars::Altars;
use crate::content::{EngineFacts, Guid};
use crate::ecs::Entity;
use crate::engine_ids as ids;
use crate::world::{ComponentType, World, WorldError};
use crate::{Load, Off};

/// The key of a map marker type, as the game's marker type registry
/// numbers them (`MapMarkerTypeId`).
///
/// # Examples
///
/// ```
/// use enshwick::content::Guid;
/// use enshwick::map::MapMarkerTypeId;
///
/// let reference = Guid::parse("5d2e6a2b-0c3f-4d8e-9a71-3e5f0b1c2d4e");
/// assert_eq!(MapMarkerTypeId::from_guid(reference).get(), 0xc807_9fa5);
/// assert_eq!(MapMarkerTypeId::new(7).get(), 7);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MapMarkerTypeId(u32);

impl MapMarkerTypeId {
    /// The type with registry key `id`.
    ///
    /// # Examples
    ///
    /// See [`MapMarkerTypeId`].
    #[must_use]
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    /// The type a marker's `markerType` reference names, keyed as the
    /// registry keys it: [`Guid::id`] of the reference.
    ///
    /// # Examples
    ///
    /// See [`MapMarkerTypeId`].
    #[must_use]
    pub const fn from_guid(reference: Guid) -> Self {
        Self(reference.id())
    }

    /// The registry key.
    ///
    /// # Examples
    ///
    /// See [`MapMarkerTypeId`].
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// A marker on the map that the world comes with: a `MapMarker` the
/// scene places.
///
/// # Examples
///
/// ```
/// use enshwick::map::{Map, MapMarker};
///
/// fn labelled(map: Map) -> Vec<MapMarker> {
///     let mut markers = map.markers().unwrap_or_default();
///     markers.retain(|marker| marker.label.is_some());
///     markers
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MapMarker {
    /// Where it is, in metres.
    pub position: [f64; 3],
    /// Its type; `None` for a marker without one.
    pub marker_type: Option<MapMarkerTypeId>,
    /// The text tag of its label, the game's `markerId`, which
    /// [`game_text`](crate::i18n::game_text) reads by [`Guid::id`] on a
    /// client; `None` for a marker without one.
    pub label: Option<Guid>,
}

/// The map's reads on the game server, dedicated or hosted by the client.
///
/// Every call belongs on the server's game thread, as for
/// [`World`]: from a game event or a
/// [`Scheduler`](crate::schedule::Scheduler) task.
///
/// # Examples
///
/// See the [module](self).
#[derive(Clone, Copy, Debug)]
pub struct Map(&'static Resolved);

/// The map's component types and field offsets, resolved once per mod life.
#[derive(Debug)]
struct Resolved {
    host: Host,
    world: World,
    waypoint: Option<Waypoint>,
}

/// Where a `PlayerWaypoint` keeps its fields.
#[derive(Debug)]
struct Waypoint {
    ty: ComponentType,
    enabled_at: usize,
    position_at: usize,
}

static RESOLVED: crate::state::ModLocal<Resolved> = crate::state::ModLocal::new();

/// The offset of field `id` of type `ty`, refused unless `width` bytes
/// from there lie inside the type, so every read of the field stays in
/// the component.
fn field(load: &Load<'_>, ty: &str, id: &str, width: usize) -> Result<usize, Off> {
    let at = load.offset(id)?;
    let size = load.size(ty)?;
    if at.checked_add(width).is_some_and(|end| end <= size) {
        Ok(at)
    } else {
        Err(Off::new(format!(
            "{id} at {at} does not hold {width} bytes in {ty} of {size}"
        )))
    }
}

impl Waypoint {
    fn resolve(load: &Load<'_>, world: World) -> Result<Self, Off> {
        Ok(Self {
            ty: world.component_type(load, ids::PLAYER_WAYPOINT)?,
            enabled_at: field(load, ids::PLAYER_WAYPOINT, ids::PLAYER_WAYPOINT_ENABLED, 1)?,
            position_at: field(
                load,
                ids::PLAYER_WAYPOINT,
                ids::PLAYER_WAYPOINT_POSITION,
                24,
            )?,
        })
    }
}

impl Map {
    pub(crate) fn resolve(load: &mut Load<'_>) -> Result<Self, Off> {
        if let Some(resolved) = RESOLVED.get() {
            return Ok(Self(resolved));
        }
        let world = load.world()?;
        let resolved = Resolved {
            host: load.host(),
            world,
            waypoint: Waypoint::resolve(load, world)
                .inspect_err(|off| crate::debug!("waypoint reads off: {off}"))
                .ok(),
        };
        Ok(Self(RESOLVED.get_or_init(|| resolved)))
    }

    /// `entity`'s component of type `ty`, `None` when it has none.
    fn live(self, entity: Entity, ty: ComponentType) -> Result<Option<*const u8>, WorldError> {
        let host = self.0.host;
        let mut out: *const core::ffi::c_void = core::ptr::null();
        let status = (host.api().component)(host.handle(), entity.0, ty.hash(), &raw mut out);
        match status {
            Status::MISSING => Ok(None),
            status => WorldError::from_status(status)
                .map(|()| Some(out.cast::<u8>()).filter(|data| !data.is_null())),
        }
    }

    /// Every marker of the scene the server runs, in the scene's order.
    /// The loader reads them from the game's data once per scene, so a
    /// call after the first is cheap.
    ///
    /// # Errors
    ///
    /// [`WorldError::WrongThread`] off the server's game thread or inside a
    /// step; [`WorldError::Missing`] while no world runs or when the scene
    /// does not read on this game build.
    ///
    /// # Examples
    ///
    /// See [`MapMarker`].
    pub fn markers(self) -> Result<Vec<MapMarker>, WorldError> {
        self.scan(|_| true)
    }

    /// The markers whose position lies in the box between corners `a` and
    /// `b`, in metres, edges included.
    ///
    /// # Errors
    ///
    /// [`WorldError::Refused`] for a corner that is not finite; otherwise
    /// as [`Map::markers`].
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::map::Map;
    ///
    /// fn near_spawn(map: Map) -> usize {
    ///     map.markers_in([-500.0, -100.0, -500.0], [500.0, 400.0, 500.0])
    ///         .map_or(0, |markers| markers.len())
    /// }
    /// ```
    pub fn markers_in(self, a: [f64; 3], b: [f64; 3]) -> Result<Vec<MapMarker>, WorldError> {
        let (low, high) = crate::world::span(a, b).ok_or(WorldError::Refused)?;
        self.scan(|at| (0..3).all(|axis| (low[axis]..=high[axis]).contains(&at[axis])))
    }

    fn scan(self, keep: impl Fn([f64; 3]) -> bool) -> Result<Vec<MapMarker>, WorldError> {
        let host = self.0.host;
        let size = u32::try_from(size_of::<MapMarkerInfo>()).map_err(|_| WorldError::Missing)?;
        let mut capacity = 1024u32;
        let infos = loop {
            let mut infos = vec![MapMarkerInfo::EMPTY; capacity as usize];
            let mut count = 0u32;
            let status = (host.api().map_markers)(
                host.handle(),
                infos.as_mut_ptr(),
                size,
                capacity,
                &raw mut count,
            );
            WorldError::from_status(status)?;
            if count <= capacity {
                infos.truncate(count as usize);
                break infos;
            }
            capacity = count;
        };
        Ok(infos
            .iter()
            .filter(|info| keep(info.position))
            .map(|info| MapMarker {
                position: info.position,
                marker_type: set(info.marker_type).map(MapMarkerTypeId::from_guid),
                label: set(info.label),
            })
            .collect())
    }

    /// Where `player` has set the waypoint all players see, in metres;
    /// `None` while it is off or for an entity without one. The server
    /// keeps it only while the player plays.
    ///
    /// # Errors
    ///
    /// [`WorldError::WrongThread`] off the server's game thread or inside a
    /// step; [`WorldError::Missing`] before a world runs or on a game build
    /// without waypoints.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    pub fn waypoint(self, player: Entity) -> Result<Option<[f64; 3]>, WorldError> {
        let waypoint = self.0.waypoint.as_ref().ok_or(WorldError::Missing)?;
        let host = self.0.host;
        WorldError::from_status((host.api().world_quiet)(host.handle()))?;
        let Some(data) = self.live(player, waypoint.ty)? else {
            return Ok(None);
        };
        // SAFETY: `data` is the player's live PlayerWaypoint, read right
        // after `world_quiet` answered that this is the game thread at a
        // quiet moment, so no system writes it now; `resolve` checked that
        // the byte at `enabled_at` lies inside the type's size. It is read as
        // a byte, never as a bool.
        let enabled = unsafe { crate::raw::read_at::<u8>(data, waypoint.enabled_at) };
        if enabled == 0 {
            return Ok(None);
        }
        // SAFETY: `data` is the player's live PlayerWaypoint, read right
        // after `world_quiet` answered that this is the game thread at a
        // quiet moment, so no system writes it now; `resolve` checked that
        // 24 bytes from `position_at`, the three i64 of `waypointPosition`
        // in 32.32 fixed point, lie inside the type.
        let fixed = unsafe { crate::raw::read_at::<[i64; 3]>(data, waypoint.position_at) };
        Ok(Some(crate::ecs::metres(fixed)))
    }
}

impl Map {
    /// What the map shows now as a [`MapSnapshot`]: every player with
    /// position and waypoint, the Flame Altars of `altars` with their
    /// bases, and every marker. Without `altars` the snapshot has none; on
    /// a game build without waypoints no player has one.
    /// The engine walks every marker, so take one now and then, not every
    /// step.
    ///
    /// # Errors
    ///
    /// As [`Map::markers`].
    ///
    /// # Examples
    ///
    /// ```
    /// use core::ops::ControlFlow;
    /// use std::time::Duration;
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let map = load.map()?;
    ///     let altars = load.altars()?;
    ///     load.scheduler()?.every(Duration::from_secs(5), move || {
    ///         if let Ok(bytes) = map.snapshot(Some(altars)).map(|s| s.to_bytes()) {
    ///             enshwick::debug!("snapshot of {} bytes", bytes.map_or(0, |b| b.len()));
    ///         }
    ///         ControlFlow::Continue(())
    ///     });
    ///     Ok(())
    /// }
    /// ```
    pub fn snapshot(self, altars: Option<Altars>) -> Result<MapSnapshot, WorldError> {
        let world = self.0.world;
        let markers = self
            .markers()?
            .into_iter()
            .map(|marker| MapPoint {
                position: marker.position,
                marker_type: marker.marker_type.map(MapMarkerTypeId::get),
                label: marker.label.map(Guid::id),
            })
            .collect();
        let mut online = world.players();
        online.sort_unstable_by_key(|player| player.slot);
        let mut players = Vec::with_capacity(online.len());
        for player in online {
            let waypoint = match self.waypoint(player.entity) {
                Err(WorldError::Missing) => None,
                found => found?,
            };
            players.push(MapPlayer {
                key: player.key,
                position: world.position(player.entity),
                waypoint,
                name: player.name,
            });
        }
        Ok(MapSnapshot {
            players,
            altars: altars.map(snapshot_altars).unwrap_or_default(),
            markers,
        })
    }
}

/// The altars of `altars`, by entity, each with its base's number.
fn snapshot_altars(altars: Altars) -> Vec<MapAltar> {
    let mut list = altars.list();
    list.sort_unstable_by_key(|altar| altar.entity.0);
    let bases = crate::altars::bases_of(&list);
    list.iter()
        .map(|altar| MapAltar {
            position: altar.position,
            level: altar.level,
            zone: [altar.zone.min, altar.zone.max],
            base: bases
                .iter()
                .position(|base| base.altars.contains(&altar.entity))
                .and_then(|index| u32::try_from(index).ok()),
        })
        .collect()
}

/// `guid`, unless it is all zero, which the game writes for none.
fn set(guid: [u8; 16]) -> Option<Guid> {
    (guid != [0; 16]).then_some(Guid(guid))
}
