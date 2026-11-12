//! The functions the loader calls to run a mod's altar filters: each copies
//! the loader's struct into the mod's type, runs the filter behind the panic
//! guard and writes its answer back.

use core::ffi::c_void;

use super::{AltarLimit, AltarZone, BuildingZoneCheck};
use crate::abi::{
    AltarLimit as RawAltarLimit, AltarZone as RawAltarZone,
    BuildingZoneCheck as RawBuildingZoneCheck,
};

pub(super) unsafe extern "system" fn run_zone<F>(zone: *mut RawAltarZone, user: *mut c_void)
where
    F: Fn(&mut AltarZone),
{
    // SAFETY: the loader passes the zone for the call, writable; `as_mut`
    // turns away null.
    let Some(raw) = (unsafe { zone.as_mut() }) else {
        return;
    };
    let mut zone = AltarZone {
        altar: raw.altar,
        level: raw.level,
        grow: raw.grow,
        height: raw.height,
    };
    // SAFETY: `user` is the leaked `F` this function was registered with.
    let filter = unsafe { &*user.cast::<F>() };
    let returned = crate::host::guard(
        "panic in an altar zone filter",
        || false,
        || {
            filter(&mut zone);
            true
        },
    );
    if returned {
        raw.grow = zone.grow;
        raw.height = zone.height;
    }
}

pub(super) unsafe extern "system" fn run_building_zone_check<F>(
    check: *const RawBuildingZoneCheck,
    user: *mut c_void,
) -> u8
where
    F: Fn(&BuildingZoneCheck<'_>) -> bool,
{
    // SAFETY: the loader passes a valid check for the duration of the call.
    let Some(check) = (unsafe { check.as_ref() }) else {
        return 0;
    };
    let zones = if check.zones.is_null() || check.zone_count == 0 {
        &[][..]
    } else {
        // SAFETY: `zone_count` boxes at `zones`, valid for the call.
        unsafe { core::slice::from_raw_parts(check.zones, check.zone_count) }
    };
    // SAFETY: `user` is the leaked `F` this function was registered with.
    let filter = unsafe { &*user.cast::<F>() };
    let call = BuildingZoneCheck {
        zones,
        object: check.object,
        margin: check.margin,
    };
    u8::from(crate::host::guard(
        "panic in a zone check",
        || false,
        || filter(&call),
    ))
}

pub(super) unsafe extern "system" fn run_limit<F>(limit: *mut RawAltarLimit, user: *mut c_void)
where
    F: Fn(&mut AltarLimit),
{
    // SAFETY: the loader passes the limit for the call, writable; `as_mut`
    // turns away null.
    let Some(raw) = (unsafe { limit.as_mut() }) else {
        return;
    };
    let mut limit = AltarLimit {
        flame_level: raw.flame_level,
        vanilla: raw.vanilla,
        allowed: raw.allowed,
    };
    // SAFETY: `user` is the leaked `F` this function was registered with.
    let filter = unsafe { &*user.cast::<F>() };
    let returned = crate::host::guard(
        "panic in an altar limit",
        || false,
        || {
            filter(&mut limit);
            true
        },
    );
    if returned {
        raw.allowed = limit.allowed;
    }
}
