//! Flame altars: their zones, sizes and the zone check.

use core::ffi::c_void;

use crate::abi::Header;

/// A box of whole cells or of 32.32 fixed-point metres, as its user says:
/// `min` inclusive, `max` exclusive on each axis; axis 1 is up.
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
pub struct Bounds {
    /// The inclusive lower corner.
    pub min: [i64; 3],
    /// The exclusive upper corner.
    pub max: [i64; 3],
}

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
/// altar.entity = 4096;
/// altar.level = 1;
/// altar.level_count = 1;
/// altar.zone = Bounds { min: [0, 0, 0], max: [80, 80, 80] };
/// altar.levels[0] = altar.zone;
/// assert_eq!(altar.header, Header::of::<AltarInfo>());
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Zeroable)]
pub struct AltarInfo {
    /// Size and version of this struct.
    pub header: Header,
    /// The altar's entity.
    pub entity: u32,
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
    /// The vanilla zone at each level from 1 to `level_count`.
    pub levels: [Bounds; 16],
    /// Where the altar stands, in metres.
    pub position: [f64; 3],
}

impl_headed!(AltarInfo);

impl AltarInfo {
    /// An altar with nothing known, its header set.
    pub const EMPTY: Self = Self {
        header: Header::of::<Self>(),
        entity: 0,
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
        }; 16],
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
/// let zone = AltarZone { header: Header::of::<AltarZone>(), altar: 4096, level: 2, reserved: [0; 3], grow: 0, height: 60 };
/// assert_eq!(zone.grow, 0);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
pub struct AltarZone {
    /// Size and version of this struct.
    pub header: Header,
    /// The altar's entity.
    pub altar: u32,
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
pub type AltarZoneFn = unsafe extern "system" fn(zone: *mut AltarZone, user: *mut c_void);

/// A placement the game refused because the object's box does not lie in a
/// single building zone, as [`HostApi::zone_check_filter`](super::HostApi::zone_check_filter) hands it on.
/// Coordinates are 32.32 fixed-point metres.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{Bounds, Header, ZoneCheck};
///
/// let check = ZoneCheck {
///     header: Header::of::<ZoneCheck>(),
///     zones: core::ptr::null(),
///     zone_count: 0,
///     object: Bounds::default(),
///     margin: 0,
/// };
/// assert_eq!(check.zone_count, 0);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
pub struct ZoneCheck {
    /// Size and version of this struct.
    pub header: Header,
    /// The building zones, half-open boxes, valid for the call.
    pub zones: *const Bounds,
    /// How many boxes `zones` holds.
    pub zone_count: usize,
    /// The object's box, both corners included.
    pub object: Bounds,
    /// How far the game lets an object reach past a zone, on every side.
    pub margin: i64,
}

impl_headed!(ZoneCheck);

/// A mod's zone check filter: the check and the pointer given at
/// registration; returns 1 to accept the placement after all.
pub type ZoneCheckFn = unsafe extern "system" fn(check: *const ZoneCheck, user: *mut c_void) -> u8;
