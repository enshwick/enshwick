//! Flame altars: their zones, sizes and the zone check.

use core::ffi::c_void;

use crate::abi::Cell;
use crate::abi::Metres;
use crate::abi::{Entity, Header};

/// A box of whole [`Cell`]s: `min` inclusive, `max` exclusive on each axis;
/// axis 1 is up.
///
/// # Examples
///
/// ```
/// use enshwick::abi::Bounds;
///
/// let zone = Bounds { min: [0, 0, 0], max: [80, 80, 80] };
/// assert_eq!(zone.max[0] - zone.min[0], 80);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, bytemuck::Zeroable)]
#[expect(
    clippy::exhaustive_structs,
    reason = "an ABI row without a header: a field of its own is a new ABI major"
)]
pub struct Bounds {
    /// The inclusive lower corner.
    pub min: Cell,
    /// The exclusive upper corner.
    pub max: Cell,
}

/// How many level zones an [`AltarInfo`] has room for: a bound of the ABI,
/// above the levels the game gives an altar in its reflected
/// `Upgrade.levels` list, which [`AltarInfo::level_count`] reports.
///
/// # Examples
///
/// ```
/// assert!(enshwick::abi::AltarInfo::EMPTY.levels.len() == enshwick::abi::ALTAR_LEVELS);
/// ```
pub const ALTAR_LEVELS: usize = 16;

/// One Flame Altar of the server's world, from [`HostApi::altars`](super::HostApi::altars).
///
/// Zones are in whole 0.5 m cells, rounded outwards, as the engine builds
/// them from the altar's trigger shape.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{AltarInfo, Bounds, Header};
///
/// let mut altar = AltarInfo::EMPTY;
/// altar.entity = enshwick::Entity::from_raw(4096).ok_or("no entity")?;
/// altar.level = 1;
/// altar.level_count = 1;
/// altar.zone = Bounds { min: [0, 0, 0], max: [80, 80, 80] };
/// altar.levels[0] = altar.zone;
/// assert_eq!(altar.header, Header::of::<AltarInfo>());
/// # Ok::<(), &str>(())
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Zeroable)]
#[non_exhaustive]
pub struct AltarInfo {
    /// Size and version of this struct.
    pub header: Header,
    /// The altar's entity.
    pub entity: Entity,
    /// Its Flame level; 0 until the engine has reported it.
    pub level: u8,
    /// 1 when the altar is tilted off the vertical: its zone is known, its
    /// level zones are not, and it keeps the engine's size.
    pub tilted: u8,
    /// How many entries of `levels` hold a zone.
    pub level_count: u8,
    /// Zero.
    pub reserved: u8,
    /// The zone as the engine has built it now, growth by any mod included.
    pub zone: Bounds,
    /// The vanilla zone at each level from 1 to `level_count`; 0 levels when
    /// the altar has more than [`ALTAR_LEVELS`].
    pub levels: [Bounds; ALTAR_LEVELS],
    /// Where the altar stands, in metres.
    pub position: Metres,
}

impl_headed!(AltarInfo);

impl AltarInfo {
    /// An altar with nothing known, its header set.
    pub const EMPTY: Self = Self {
        header: Header::of::<Self>(),
        entity: Entity::NONE,
        level: 0,
        tilted: 0,
        level_count: 0,
        reserved: 0,
        zone: Bounds {
            min: [0; 3],
            max: [0; 3],
        },
        levels: [Bounds {
            min: [0; 3],
            max: [0; 3],
        }; ALTAR_LEVELS],
        position: [0.0; 3],
    };
}

/// One altar's zone as the loader sizes it, handed to every altar zone
/// filter in load order.
///
/// The zone is the altar's vanilla zone at `level` grown by `grow` cells on
/// every horizontal side and made `height` cells tall, as the filters before
/// have left it; see
/// [`HostApi::altar_zone_filter`](super::HostApi::altar_zone_filter).
///
/// # Examples
///
/// ```
/// use enshwick::abi::{AltarZone, Header};
///
/// let zone = enshwick::abi::headed!(AltarZone { altar: enshwick::Entity::from_raw(4096).ok_or("no entity")?, level: 2, reserved: [0; 3], grow: 0, height: 60 });
/// assert_eq!(zone.grow, 0);
/// # Ok::<(), &str>(())
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
#[non_exhaustive]
pub struct AltarZone {
    /// Size and version of this struct.
    pub header: Header,
    /// The altar's entity.
    pub altar: Entity,
    /// The altar's level.
    pub level: u8,
    /// Zero.
    pub reserved: [u8; 3],
    /// Cells added on every horizontal side, at least 0; 0 for vanilla.
    pub grow: i64,
    /// The zone's height in cells; the vanilla height for vanilla.
    pub height: i64,
}

impl_headed!(AltarZone);

/// An altar zone filter: the zone so far, writable, and the pointer given
/// at registration. It must not unwind.
///
/// Null is never a callback: the loader refuses a null one with
/// [`Status::BAD_ARGS`](crate::abi::Status::BAD_ARGS) and keeps nothing.
/// Runs on the thread of the game system that asks, the game thread or a task
/// worker, so several calls may run at once.
///
/// # Examples
///
/// The safe layer wraps it in [`Load::on_altar_zone`](../struct.Load.html#method.on_altar_zone).
pub type AltarZoneFilterFn = unsafe extern "system" fn(zone: *mut AltarZone, user: *mut c_void);

/// A placement the game refused because the object's box does not lie in a
/// single building zone, as [`HostApi::building_zone_filter`](super::HostApi::building_zone_filter) hands it on.
/// Coordinates are [`Metres`].
///
/// The name is enshwick's, after the game's building zone: the game's check
/// has no name of its own.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{BuildingZoneCheck, Extent, Header};
///
/// let check = enshwick::abi::headed!(BuildingZoneCheck {
///     zones: core::ptr::null(),
///     zone_count: 0,
///     object: Extent::default(),
///     margin: 0.0,
/// });
/// assert_eq!(check.zone_count, 0);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
#[non_exhaustive]
pub struct BuildingZoneCheck {
    /// Size and version of this struct.
    pub header: Header,
    /// The building zones, valid for the call.
    pub zones: *const Extent,
    /// How many boxes `zones` holds.
    pub zone_count: u32,
    /// The object's box.
    pub object: Extent,
    /// How far the game lets an object reach past a zone on every side, in metres.
    pub margin: f64,
}

impl_headed!(BuildingZoneCheck);

/// A box in the world, from its lowest corner `min` to its highest `max`, in
/// [`Metres`].
///
/// # Examples
///
/// ```
/// use enshwick::abi::Extent;
///
/// let room = Extent { min: [0.0; 3], max: [4.0, 3.0, 4.0] };
/// assert!(room.max[1] > room.min[1]);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, bytemuck::Zeroable)]
#[expect(
    clippy::exhaustive_structs,
    reason = "an ABI row without a header: a field of its own is a new ABI major"
)]
pub struct Extent {
    /// The lowest corner.
    pub min: Metres,
    /// The highest corner, at least `min` on every axis.
    pub max: Metres,
}

/// A mod's zone check filter: the check and the pointer given at
/// registration; returns [`FILTER_YES`](super::FILTER_YES) to accept the
/// placement after all, and any other answer keeps the game's refusal.
///
/// Null is never a callback: the loader refuses a null one with
/// [`Status::BAD_ARGS`](crate::abi::Status::BAD_ARGS) and keeps nothing.
/// Runs on the thread of the game system that asks, the game thread or a task
/// worker, so several calls may run at once.
///
/// # Examples
///
/// The safe layer wraps it in
/// [`Load::on_building_zone_check`](../struct.Load.html#method.on_building_zone_check).
pub type BuildingZoneFilterFn =
    unsafe extern "system" fn(check: *const BuildingZoneCheck, user: *mut c_void) -> u8;
