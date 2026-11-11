//! The fake's altar entries: the altars, their zones and limits, and the
//! base limit.

use core::ffi::c_void;

use crate::test::BaseCode;
use crate::test::table::base::mods::list;
use crate::test::table::{Registered, put, with};
use enshwick_core::abi::{
    AltarInfo, AltarLimitFilterFn, AltarZoneFn, BaseLimitInfo, BuildingZoneCheckFn, Header,
    ModHandle, Status,
};

/// The base limits the mod asked for, and whether the hooks are armed.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct BaseLimitState {
    /// The largest zones and bases the mod asked for, once it asked.
    pub(crate) requested: Option<(u32, u32)>,
    /// The mod's init returned and its hooks are armed.
    pub(crate) armed: bool,
    /// The mod's latest base cap, once it set one.
    pub(crate) cap: Option<u32>,
}

/// The mod's altar zone filters, and how often it asked for a new sizing.
#[derive(Debug, Default)]
pub(crate) struct ZoneSizing {
    /// The filters and their pointers.
    pub(crate) filters: Vec<Registered<AltarZoneFn>>,
    /// How often the mod asked for the zones to be sized again.
    pub(crate) resizes: usize,
}

pub(crate) extern "system" fn altars_open(_me: ModHandle) -> Status {
    with(Status::MISSING, |s| {
        s.altars_opened = true;
        Status::OK
    })
}

pub(crate) extern "system" fn altars(
    _me: ModHandle,
    out: *mut AltarInfo,
    entry_size: u32,
    capacity: u32,
    count: *mut u32,
) -> Status {
    let Some(altars) = with(None, |s| s.altars_opened.then(|| s.altars.clone())) else {
        return Status::REFUSED;
    };
    list(&altars, out, entry_size, capacity, count)
}

pub(crate) extern "system" fn altar_zone_filter(
    me: ModHandle,
    callback: AltarZoneFn,
    user: *mut c_void,
) -> Status {
    with(Status::MISSING, |s| {
        s.altars_opened = true;
        s.zone_control = true;
        s.zone_sizing
            .filters
            .push(Registered::new(callback, user, me));
        Status::OK
    })
}

pub(crate) extern "system" fn altar_zones_changed(_me: ModHandle) -> Status {
    with(Status::MISSING, |s| {
        if !s.zone_control {
            return Status::REFUSED;
        }
        s.zone_sizing.resizes += 1;
        Status::OK
    })
}

pub(crate) extern "system" fn building_zone_check_filter(
    me: ModHandle,
    callback: BuildingZoneCheckFn,
    user: *mut c_void,
) -> Status {
    with(Status::MISSING, |s| {
        s.zone_filters.push(Registered::new(callback, user, me));
        Status::OK
    })
}

pub(crate) extern "system" fn altar_limit_filter(
    me: ModHandle,
    callback: AltarLimitFilterFn,
    user: *mut c_void,
) -> Status {
    with(Status::MISSING, |s| {
        s.altar_limits.push(Registered::new(callback, user, me));
        Status::OK
    })
}

/// The altar zones and bases the fake's game groups by itself, as the
/// game's.
const VANILLA: (u32, u32) = (16, 16);
/// The most altar zones and bases a mod may ask for, as the loader's.
const MOST: (u32, u32) = (255, 179);
/// What raising the base limit asks for.
const RAISED: (u32, u32) = (32, 16);

/// The zones and bases the fake's game groups, as the loader's: the game's
/// own, or the mod's largest request once its hooks are armed.
pub(crate) fn zones_and_bases() -> (u32, u32) {
    let most = most();
    with(None, |s| {
        s.base_limit.requested.filter(|_| s.base_limit.armed)
    })
    .map_or(VANILLA, |(zones, bases)| {
        let cap = with(None, |s| s.base_limit.cap).unwrap_or(u32::MAX);
        (zones, bases.min(most.1).min(cap))
    })
}

/// The most a mod may ask for, as the loader's: its own most, or 16 bases
/// once arming finds a build that keeps them.
fn most() -> (u32, u32) {
    let kept = with(false, |s| {
        s.base_limit.armed && s.host.base_code == BaseCode::ZonesOnly
    });
    if kept { (MOST.0, VANILLA.1) } else { MOST }
}

/// Takes a request for `zones` and `bases`, as the loader does: refused
/// past the most it takes or after init, kept as the largest of each.
fn request(zones: u32, bases: u32) -> Status {
    if zones > MOST.0 || bases > MOST.1 {
        return Status::BAD_ARGS;
    }
    with(Status::MISSING, |s| {
        if s.base_limit.armed {
            return Status::TOO_LATE;
        }
        if s.host.base_code == BaseCode::Unknown {
            return Status::MISSING;
        }
        let (old_zones, old_bases) = s.base_limit.requested.unwrap_or(VANILLA);
        s.base_limit.requested = Some((
            zones.max(old_zones).max(VANILLA.0),
            bases.max(old_bases).max(VANILLA.1),
        ));
        Status::OK
    })
}

pub(crate) extern "system" fn base_limit(_me: ModHandle, out: *mut BaseLimitInfo) -> Status {
    let (zones, bases) = zones_and_bases();
    let (max_zones, max_bases) = most();
    let limit = BaseLimitInfo {
        header: Header::of::<BaseLimitInfo>(),
        zones,
        bases,
        vanilla_zones: VANILLA.0,
        reserved: 0,
        vanilla_bases: VANILLA.1,
        max_zones,
        max_bases,
    };
    // SAFETY: the safe layer passes `out` null or a `BaseLimitInfo` with
    // its header set, writable for the call.
    if unsafe { enshwick_core::abi::write_headed(out, limit) } {
        Status::OK
    } else {
        Status::BAD_ARGS
    }
}

pub(crate) extern "system" fn raise_base_limit(_me: ModHandle) -> Status {
    request(RAISED.0, RAISED.1)
}

pub(crate) extern "system" fn request_base_limits(
    _me: ModHandle,
    zones: u32,
    bases: u32,
) -> Status {
    request(zones, bases)
}

pub(crate) extern "system" fn set_base_cap(_me: ModHandle, bases: u32) -> Status {
    if bases < VANILLA.1 {
        return Status::BAD_ARGS;
    }
    with(Status::MISSING, |s| {
        s.base_limit.cap = Some(bases);
        Status::OK
    })
}

pub(crate) extern "system" fn base_zones(
    _me: ModHandle,
    zones: *mut u32,
    bases: *mut u32,
) -> Status {
    let Some((zone_count, base_count)) = with(None, |s| s.grouping) else {
        return Status::MISSING;
    };
    let status = put(zones, zone_count);
    if !status.is_ok() {
        return status;
    }
    put(bases, base_count)
}
