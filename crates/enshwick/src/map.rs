//! The world's map as the server knows it: its markers and each player's
//! waypoint.
//!
//! A marker is a `MapMarker` the world comes with: a town, a ruin, a
//! dungeon, a Flame shrine, a quest area. Its type is a key into the game's
//! marker type registry, an open set the game's data defines. What a player
//! has uncovered of the map lives only on that player's machine.
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
//!             if let Some(at) = map.waypoint(player.entity) {
//!                 enshwick::info!("{} marks {at:?}", player.name);
//!             }
//!         }
//!         ControlFlow::Continue(())
//!     });
//!     Ok(())
//! }
//! ```

use crate::content::Guid;
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
/// assert_eq!(MapMarkerTypeId::of(reference).get(), reference.id());
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
    /// registry keys it.
    ///
    /// # Examples
    ///
    /// See [`MapMarkerTypeId`].
    #[must_use]
    pub const fn of(reference: Guid) -> Self {
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

/// A marker on the map that the world comes with, from its `MapMarker`.
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
    /// The entity that carries it.
    pub entity: Entity,
    /// Where it is, in metres.
    pub position: [f64; 3],
    /// Its type; `None` for a marker without one.
    pub marker_type: Option<MapMarkerTypeId>,
    /// The text tag of its label, which
    /// [`game_text`](crate::i18n::game_text) reads by
    /// [`Guid::id`] on a client; `None` for a marker without one.
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
    world: World,
    marker: ComponentType,
    label_at: usize,
    type_at: usize,
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

fn offset(load: &Load<'_>, id: &str) -> Result<usize, Off> {
    let value = load.constant(id)?;
    usize::try_from(value).map_err(|_| Off::new(format!("offset {id} out of range")))
}

impl Waypoint {
    fn resolve(load: &Load<'_>, world: World) -> Result<Self, Off> {
        Ok(Self {
            ty: world.component_type(load, ids::PLAYER_WAYPOINT)?,
            enabled_at: offset(load, ids::PLAYER_WAYPOINT_ENABLED)?,
            position_at: offset(load, ids::PLAYER_WAYPOINT_POSITION)?,
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
            world,
            marker: world.component_type(load, ids::MAP_MARKER)?,
            label_at: offset(load, ids::MAP_MARKER_LABEL)?,
            type_at: offset(load, ids::MAP_MARKER_TYPE)?,
            waypoint: Waypoint::resolve(load, world)
                .inspect_err(|off| crate::debug!("waypoint reads off: {off}"))
                .ok(),
        };
        Ok(Self(RESOLVED.get_or_init(|| resolved)))
    }

    /// Every marker in the world, by entity. The engine walks every entity
    /// with a `MapMarker`, so call it now and then, not every step.
    ///
    /// # Errors
    ///
    /// [`WorldError::WrongThread`] off the server's game thread or inside a
    /// step; [`WorldError::Missing`] before a world runs.
    ///
    /// # Examples
    ///
    /// See [`MapMarker`].
    pub fn markers(self) -> Result<Vec<MapMarker>, WorldError> {
        self.scan(|_| true)
    }

    /// The markers whose position lies in the box between corners `a` and
    /// `b`, in metres, edges included, by entity.
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
        let resolved = self.0;
        let world = resolved.world;
        let mut markers: Vec<MapMarker> = world
            .placed_with(resolved.marker.hash())?
            .iter()
            .map(|place| (Entity(place.entity), crate::ecs::metres(place.position)))
            .filter(|&(_, at)| keep(at))
            .filter_map(|(entity, position)| {
                let data = world.component(entity, resolved.marker)?.cast::<u8>();
                // SAFETY: `component` returned the entity's live MapMarker,
                // valid until the game thread moves on and read at once; the
                // loader's offsets of `markerId` and `markerType` lie inside
                // it, each a 16-byte GUID (reflection, server 399).
                let label = unsafe { crate::raw::read_at::<[u8; 16]>(data, resolved.label_at) };
                // SAFETY: `data` is the entity's live MapMarker from
                // `component`, read before the game thread moves on;
                // `markerType` is a 16-byte GUID at the loader's offset,
                // inside the component (reflection, server 399).
                let reference = unsafe { crate::raw::read_at::<[u8; 16]>(data, resolved.type_at) };
                Some(MapMarker {
                    entity,
                    position,
                    marker_type: set(reference).map(MapMarkerTypeId::of),
                    label: set(label),
                })
            })
            .collect();
        markers.sort_unstable_by_key(|marker| marker.entity.0);
        Ok(markers)
    }

    /// Where `player` has set the waypoint all players see, in metres;
    /// `None` while it is off, for an entity without one, or on a game
    /// build without it. The server keeps it only while the player plays.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    #[must_use]
    pub fn waypoint(self, player: Entity) -> Option<[f64; 3]> {
        let waypoint = self.0.waypoint.as_ref()?;
        let data = self.0.world.component(player, waypoint.ty)?.cast::<u8>();
        // SAFETY: `component` returned the player's live PlayerWaypoint,
        // valid until the game thread moves on and read at once; `enabled`
        // is one byte at the loader's offset, read as a byte, never a bool.
        let enabled = unsafe { crate::raw::read_at::<u8>(data, waypoint.enabled_at) };
        if enabled == 0 {
            return None;
        }
        // SAFETY: the same live PlayerWaypoint; `waypointPosition` is a
        // WorldPosition, three i64 in 32.32 fixed point, at the loader's
        // offset (reflection, server 399).
        let fixed = unsafe { crate::raw::read_at::<[i64; 3]>(data, waypoint.position_at) };
        Some(crate::ecs::metres(fixed))
    }
}

/// `guid`, unless it is all zero, which the game writes for none.
fn set(guid: [u8; 16]) -> Option<Guid> {
    (guid != [0; 16]).then_some(Guid(guid))
}
