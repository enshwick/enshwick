//! The altar zones service: what `flame-altar-zones` publishes for other mods.
//!
//! This file is the whole contract. `survivors-per-altar` compiles the same
//! file into itself (`#[path]`), so both sides always declare one table; it
//! uses nothing but `core`, and its layout is checked at compile time.

/// One altar's zone as the engine builds it now, in whole cells.
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

/// The service table, published as [`NAME`] version [`VERSION`].
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
