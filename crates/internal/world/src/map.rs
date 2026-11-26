//! The world's map as the server knows it: its markers, its frame and
//! each player's waypoint.
//!
//! Unstable: needs the `unstable` feature and may change in any release.
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

pub(crate) mod events;
mod fog;
mod snapshot;

pub use events::{FogOfWarUncovered, MapPinged, WaypointChanged};
pub use fog::{FogError, FogImage, FogOfWar, FogState};

pub use snapshot::{
    MAP_SNAPSHOT_FORMAT, MapAltar, MapPlayer, MapPoint, MapSnapshot, SnapshotError,
};

use crate::__load::*;
use crate::BaseLoad;
use crate::Host;
use crate::Off;
use crate::WorldError;
use crate::abi::{MapFrameInfo, MapMarkerInfo, WaypointData};
use crate::altars::Altars;
use crate::content::Guid;
use crate::world::{PlayerRef, World};

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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, crate::Script)]
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
    /// registry keys it: [`Guid::fnv1a`] of the reference.
    ///
    /// # Examples
    ///
    /// See [`MapMarkerTypeId`].
    #[must_use]
    pub const fn from_guid(reference: Guid) -> Self {
        Self(reference.fnv1a())
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
#[derive(Clone, Copy, Debug, PartialEq, crate::Script)]
#[non_exhaustive]
pub struct MapMarker {
    /// Where it is, in metres.
    pub position: [f64; 3],
    /// Its type; `None` for a marker without one.
    pub marker_type: Option<MapMarkerTypeId>,
    /// The text tag of its label, the game's `markerId`, which
    /// [`game_text`](crate::i18n::game_text) reads by [`Guid::fnv1a`] on a
    /// client; `None` for a marker without one.
    pub label: Option<Guid>,
}

impl MapMarker {
    /// A `MapMarker` of its parts; `marker_type` and `label` start as `None`.
    ///
    /// # Examples
    ///
    /// See [`MapMarker`].
    #[must_use]
    pub const fn new(position: [f64; 3]) -> Self {
        Self {
            position,
            marker_type: None,
            label: None,
        }
    }
}

/// The group the client's map sorts a marker type into, the game's
/// `MarkerTypeSorting`.
///
/// # Examples
///
/// See [`MapMarkerType`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, crate::Script)]
#[non_exhaustive]
pub enum MarkerTypeSorting {
    /// In no group.
    None,
    /// The Flame's: altars and shrines.
    FlameRelated,
    /// Dungeons.
    Dungeons,
    /// Places in the world.
    Locations,
    /// Markers players place themselves.
    CustomMarker,
}

impl MarkerTypeSorting {
    fn from_raw(raw: crate::abi::MarkerTypeSorting) -> Option<Self> {
        use crate::abi::MarkerTypeSorting as Raw;
        Some(match raw {
            Raw::NONE => Self::None,
            Raw::FLAME_RELATED => Self::FlameRelated,
            Raw::DUNGEONS => Self::Dungeons,
            Raw::LOCATIONS => Self::Locations,
            Raw::CUSTOM_MARKER => Self::CustomMarker,
            _ => return None,
        })
    }
}

/// A marker type of the client's marker type registry, the game's
/// `MapMarkerType`: what a marker of that type looks like on the map,
/// for a mod's own markers to take one.
///
/// # Examples
///
/// ```
/// use enshwick::map::{MapMarkerType, MarkerTypeSorting};
/// use enshwick::ui::Ui;
///
/// fn fast_travel(ui: Ui) -> Vec<MapMarkerType> {
///     let mut types = ui.map_marker_types().unwrap_or_default();
///     types.retain(|ty| ty.is_fast_travel_destination);
///     types.sort_by_key(|ty| ty.sorting_category != Some(MarkerTypeSorting::FlameRelated));
///     types
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, crate::Script)]
#[non_exhaustive]
#[expect(
    clippy::struct_excessive_bools,
    reason = "the game's own flags of a marker type, each named as it names them"
)]
pub struct MapMarkerType {
    /// Its key, `markerId`, which [`MapMarker::marker_type`] names and
    /// [`Marker::marker_type`](../ui/struct.Marker.html#method.marker_type) takes.
    pub marker_type: MapMarkerTypeId,
    /// The text tag of its name, `typeName`; `None` for most types, which
    /// have none.
    pub type_name: Option<Guid>,
    /// The group the map sorts it into; `None` for a group the game added
    /// that this API does not know yet.
    pub sorting_category: Option<MarkerTypeSorting>,
    /// Whether players travel to markers of this type.
    pub is_fast_travel_destination: bool,
    /// Whether it is a type of the markers players place themselves.
    pub is_custom_marker_type: bool,
    /// Whether its markers show over the fog of war.
    pub show_above_fog_of_war: bool,
    /// Whether its markers show their label.
    pub show_label: bool,
}

impl MapMarkerType {
    pub(crate) fn from_info(info: &crate::abi::MapMarkerTypeInfo) -> Self {
        Self {
            marker_type: MapMarkerTypeId::new(info.marker_id),
            type_name: set(info.type_name),
            sorting_category: MarkerTypeSorting::from_raw(info.sorting_category),
            is_fast_travel_destination: info.is_fast_travel_destination != 0,
            is_custom_marker_type: info.is_custom_marker_type != 0,
            show_above_fog_of_war: info.show_above_fog_of_war != 0,
            show_label: info.show_label != 0,
        }
    }
}

/// Where the map of the world starts and how far it reaches, in metres on
/// the world's x and z axes. The name is Enshwick's own: the game keeps
/// these facts with the scene's biome map.
///
/// A map image or grid drawn over the world puts its corner at `origin` and
/// spans `size`; the game's own map divides it into cells of
/// `metres_per_cell` on each side.
///
/// # Examples
///
/// ```
/// use enshwick::map::MapFrame;
///
/// let frame = MapFrame::new([0.0, 0.0], [10240.0, 10240.0], 8.0);
/// let cells = frame.size.map(|metres| metres / frame.metres_per_cell);
/// assert_eq!(cells, [1280.0, 1280.0]);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize, crate::Script)]
#[non_exhaustive]
pub struct MapFrame {
    /// The map's corner of lowest x and z, in metres, x then z.
    pub origin: [f64; 2],
    /// How far the map reaches from `origin`, in metres, x then z.
    pub size: [f64; 2],
    /// The metres one cell of the game's map covers on each axis.
    pub metres_per_cell: f64,
}

impl MapFrame {
    /// A `MapFrame` of its parts.
    ///
    /// # Examples
    ///
    /// See [`MapFrame`].
    #[must_use]
    pub const fn new(origin: [f64; 2], size: [f64; 2], metres_per_cell: f64) -> Self {
        Self {
            origin,
            size,
            metres_per_cell,
        }
    }
}

impl MapFrame {
    /// The frame `info` describes, refused unless every value is finite and
    /// the size and cell are above zero.
    fn from_info(info: &MapFrameInfo) -> Option<Self> {
        let finite = info
            .origin
            .iter()
            .chain(&info.size)
            .chain([&info.metres_per_cell])
            .all(|value| value.is_finite());
        let positive = info
            .size
            .iter()
            .chain([&info.metres_per_cell])
            .all(|&value| value > 0.0);
        (finite && positive).then_some(Self {
            origin: info.origin,
            size: info.size,
            metres_per_cell: info.metres_per_cell,
        })
    }
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

/// `Load`'s methods of this module on the base handle, which core's
/// forwarders call.
#[doc(hidden)]
pub trait LoadMethods {
    /// As `Load::map`.
    #[doc(hidden)]
    fn map(&mut self) -> Result<Map, Off>;

    /// As `Load::fog_of_war`.
    #[doc(hidden)]
    fn fog_of_war(&mut self) -> Result<FogOfWar, Off>;
}

impl crate::script::Bound for Map {
    const NAME: &'static str = "map";
    const DOC: &'static str =
        "The world's map on the server: its markers, its frame, waypoints and snapshots.";
    const PARTS: &'static [&'static [crate::script::Function<Self>]] =
        &[Self::SCRIPT_MARKERS, Self::SCRIPT_SNAPSHOT];

    fn resolve(load: &mut impl crate::Loading) -> Result<Self, Off> {
        load.base_mut().map()
    }
}

impl crate::script::Bound for FogOfWar {
    const NAME: &'static str = <Map as crate::script::Bound>::NAME;
    const DOC: &'static str = <Map as crate::script::Bound>::DOC;
    // Only `map.fog_of_war` names it there; its functions are the methods of
    // the object that returns, as `FogOfWar`'s are in Rust, not `map`'s.
    const PARTS: &'static [&'static [crate::script::Function<Self>]] = &[];

    fn resolve(load: &mut impl crate::Loading) -> Result<Self, Off> {
        load.base_mut().fog_of_war()
    }
}

impl LoadMethods for BaseLoad {
    fn map(&mut self) -> Result<Map, Off> {
        Map::resolve(self)
    }

    fn fog_of_war(&mut self) -> Result<FogOfWar, Off> {
        FogOfWar::open(self)
    }
}

/// What the map reads through, the host and the world, taken once per mod life.
#[derive(Debug)]
struct Resolved {
    host: Host,
    world: World,
}

static RESOLVED: crate::state::ModLocal<Resolved> = crate::state::ModLocal::new();

#[crate::script(markers)]
impl Map {
    pub(crate) fn resolve(load: &mut BaseLoad) -> Result<Self, Off> {
        if let Some(resolved) = RESOLVED.get() {
            return Ok(Self(resolved));
        }
        let world = load.world()?;
        let resolved = Resolved {
            host: load.host(),
            world,
        };
        Ok(Self(RESOLVED.get_or_init(|| resolved)))
    }

    /// Every marker of the scene the server runs, in the scene's order.
    /// The loader reads them from the game's data once per scene, apart
    /// from the game thread, so a call after that is cheap.
    ///
    /// # Errors
    ///
    /// [`WorldError::NotLoaded`] while the loader still reads the scene,
    /// a second or two after the first call: ask again later;
    /// [`WorldError::WrongThread`] off the server's game thread or inside a
    /// step; [`WorldError::Unavailable`] while no world runs or when the scene
    /// does not read on this game build.
    ///
    /// # Examples
    ///
    /// See [`MapMarker`].
    #[script]
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
    #[script]
    pub fn markers_in(self, a: [f64; 3], b: [f64; 3]) -> Result<Vec<MapMarker>, WorldError> {
        let (low, high) = crate::world::span(a, b).ok_or(WorldError::Refused)?;
        self.scan(|at| (0..3).all(|axis| (low[axis]..=high[axis]).contains(&at[axis])))
    }

    fn scan(self, keep: impl Fn([f64; 3]) -> bool) -> Result<Vec<MapMarker>, WorldError> {
        let host = self.0.host;
        let size =
            u32::try_from(size_of::<MapMarkerInfo>()).map_err(|_| WorldError::Unavailable)?;
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

    /// The frame of the map of the scene the server runs: where the map
    /// starts, how far it reaches and the size of its cells, in metres. The
    /// loader reads it from the game's data once per scene, apart from the
    /// game thread, as it reads the markers.
    ///
    /// # Errors
    ///
    /// [`WorldError::NotLoaded`] while the loader still reads the scene:
    /// ask again later; [`WorldError::WrongThread`] off the server's game
    /// thread or inside a step; [`WorldError::Unavailable`] while no world
    /// runs, for a scene without a map, or when the scene does not read on
    /// this game build.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::map::Map;
    ///
    /// fn cells(map: Map) -> Option<[f64; 2]> {
    ///     let frame = map.frame().ok()?;
    ///     Some(frame.size.map(|metres| metres / frame.metres_per_cell))
    /// }
    /// ```
    #[script]
    pub fn frame(self) -> Result<MapFrame, WorldError> {
        let host = self.0.host;
        let mut info = MapFrameInfo::EMPTY;
        WorldError::from_status((host.api().map_frame)(host.handle(), &raw mut info))?;
        MapFrame::from_info(&info).ok_or(WorldError::Unavailable)
    }

    /// Uncovers the fog of war of `player` in the circle of `radius` metres
    /// around `at` on the footprint, its height ignored, as exploring there does: all of the
    /// circle, with the fog thinning out from its edge to twice the radius.
    /// The server holds
    /// no fog: the loader sends the circle to the loader on the player's
    /// client, which hands it to the game's own reveal within a frame, and
    /// the character's save keeps it, in every world the character enters.
    /// On a client that hosts, the hosting player's own fog uncovers on that
    /// client, with no message. A client without Enshwick is not reached,
    /// and the mod decides what to tell whoever asked. Returns once the
    /// circle is sent; any thread, on a dedicated server or a client that
    /// hosts.
    ///
    /// # Errors
    ///
    /// [`WorldError::Unavailable`] for a player whose client takes no messages
    /// from the loader, as one without Enshwick or not online, and on a
    /// client in another's game; [`WorldError::Refused`] for a place that
    /// is not finite, a radius that is not above 0 and at most 100 000
    /// metres, or a link to the player with no room; [`WorldError::Gone`]
    /// for a character no connected player has.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::map::Map;
    ///
    /// fn show_the_shrine(map: Map, player: &enshwick::world::Player) {
    ///     if let Err(why) = map.uncover(player, [1250.0, 0.0, -830.0], 200.0) {
    ///         enshwick::warn!("fog not uncovered: {why}");
    ///     }
    /// }
    /// ```
    #[script]
    pub fn uncover(
        self,
        player: impl Into<PlayerRef>,
        at: [f64; 3],
        radius: f32,
    ) -> Result<(), WorldError> {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "the game's reveal takes the place as f32, as its world is far smaller than f32's range"
        )]
        let (x, z) = (at[0] as f32, at[2] as f32);
        let host = self.0.host;
        let player = player.into().key_in(host)?;
        WorldError::from_status((host.api().fog_uncover)(
            host.handle(),
            player,
            x,
            z,
            radius,
        ))
    }

    /// Uncovers the whole map of `player`, as [`uncover`](Self::uncover)
    /// does for one circle: the circle around the map's [`frame`](Self::frame)
    /// that holds all of it. On the server's game thread, as `frame` is.
    ///
    /// # Errors
    ///
    /// As [`frame`](Self::frame) and [`uncover`](Self::uncover).
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::map::Map;
    ///
    /// fn reveal_all(map: Map, player: enshwick::PlayerKey) -> Result<(), enshwick::WorldError> {
    ///     map.uncover_all(player)
    /// }
    /// ```
    #[script]
    pub fn uncover_all(self, player: impl Into<PlayerRef>) -> Result<(), WorldError> {
        let frame = self.frame()?;
        let centre = [
            frame.origin[0] + frame.size[0] / 2.0,
            0.0,
            frame.origin[1] + frame.size[1] / 2.0,
        ];
        #[expect(
            clippy::cast_possible_truncation,
            reason = "a map's half diagonal in metres fits f32 by far"
        )]
        let radius = (frame.size[0].hypot(frame.size[1]) / 2.0) as f32;
        self.uncover(player, centre, radius)
    }

    /// Where `player` has set the waypoint all players see, in metres;
    /// `None` while it is off or for an entity without one. The server
    /// keeps it only while the player plays.
    ///
    /// # Errors
    ///
    /// [`WorldError::WrongThread`] off the server's game thread or inside a
    /// step; [`WorldError::Unavailable`] before a world runs or on a game build
    /// without waypoints; [`WorldError::Gone`] for an account no connected
    /// player has.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    #[script]
    pub fn waypoint(self, player: impl Into<PlayerRef>) -> Result<Option<[f64; 3]>, WorldError> {
        let host = self.0.host;
        let player = player.into().entity_in(host)?;
        let mut data = WaypointData::EMPTY;
        WorldError::from_status((host.api().waypoint)(host.handle(), player, &raw mut data))?;
        Ok((data.set != 0).then_some(data.position))
    }
}

#[crate::script(snapshot)]
impl Map {
    /// What the map shows now as a [`MapSnapshot`]: every player with
    /// position and waypoint, the Flame Altars of `altars` with their
    /// bases, every marker and the map's frame. Without `altars` the
    /// snapshot has none; without a frame its `frame` is `None`; on
    /// a game build without waypoints no player has one.
    /// The loader reads every marker, so take one now and then, not every
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
    #[script]
    pub fn snapshot(self, altars: Option<Altars>) -> Result<MapSnapshot, WorldError> {
        let world = self.0.world;
        let frame = match self.frame() {
            Err(WorldError::Unavailable) => Ok(None),
            found => found.map(Some),
        };
        let markers = self.markers();
        let frame = frame?;
        let markers = markers?
            .into_iter()
            .map(|marker| MapPoint {
                position: marker.position,
                marker_type: marker.marker_type.map(MapMarkerTypeId::get),
                label: marker.label.map(Guid::fnv1a),
            })
            .collect();
        let mut online = world.players();
        online.sort_unstable_by_key(|player| player.slot);
        let mut players = Vec::with_capacity(online.len());
        for player in online {
            let waypoint = match self.waypoint(player.entity) {
                Err(WorldError::Unavailable) => None,
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
            altars: altars.map_or_default(snapshot_altars),
            markers,
            frame,
        })
    }
}

/// The altars of `altars`, by entity, each with its base's number.
fn snapshot_altars(altars: Altars) -> Vec<MapAltar> {
    let mut list = altars.list();
    list.sort_unstable_by_key(|altar| altar.entity.get());
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
fn set(guid: Guid) -> Option<Guid> {
    (guid != Guid::NIL).then_some(guid)
}
