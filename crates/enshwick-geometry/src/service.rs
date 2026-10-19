//! The altar zones service: what `flame-altar-zones` publishes for other mods.

/// One altar's zone as the engine builds it now, in whole cells.
///
/// # Examples
///
/// ```
/// use enshwick_geometry::service::AltarZone;
///
/// let zone = AltarZone { altar: 1026, min: [0, 0, 0], max: [80, 80, 80] };
/// assert_eq!(zone.to_box().footprint_area(), 6400);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AltarZone {
    /// The altar's entity.
    pub altar: u32,
    /// The inclusive lower corner.
    pub min: [i64; 3],
    /// The exclusive upper corner.
    pub max: [i64; 3],
}

impl AltarZone {
    /// The zone as a [`Box3`](crate::Box3).
    ///
    /// # Examples
    ///
    /// See [`AltarZone`].
    #[must_use]
    pub const fn to_box(self) -> crate::Box3 {
        crate::Box3::new(self.min, self.max)
    }
}

/// The service table, published as [`NAME`] version [`VERSION`].
///
/// # Examples
///
/// ```
/// use enshwick_geometry::service::{AltarZone, ZonesService};
///
/// extern "system" fn none(_out: *mut AltarZone, _capacity: usize) -> usize {
///     0
/// }
///
/// let service = ZonesService { zones: none };
/// assert_eq!((service.zones)(core::ptr::null_mut(), 0), 0);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct ZonesService {
    /// Writes up to `capacity` zones to `out` and returns how many altars have
    /// a zone, which may exceed `capacity`.
    pub zones: extern "system" fn(out: *mut AltarZone, capacity: usize) -> usize,
}

/// The service's name.
pub const NAME: &str = "flame-altar-zones.zones";
/// The service's version; a new incompatible table is a new version.
pub const VERSION: u32 = 1;

const _: () = {
    assert!(size_of::<AltarZone>() == 56);
    assert!(size_of::<ZonesService>() == 8);
};
