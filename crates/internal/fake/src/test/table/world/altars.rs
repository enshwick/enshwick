//! The fake's altar entries: the altars, their zones and limits, and the
//! base limit.

use crate::test::table::base::args;
use crate::test::table::entries;
use core::ffi::c_void;

use crate::test::table::base::io::list;
use crate::test::table::{Registered, put, with};
use crate::test::world::BaseCode;
use enshwick_core::abi::{
    AltarInfo, AltarLimitFilterFn, AltarZoneFilterFn, BaseLimitInfo, BuildingZoneFilterFn, Header,
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

/// Adds `me` to the mods that opened a part, once.
fn opened(mods: &mut Vec<ModHandle>, me: ModHandle) {
    if !mods.contains(&me) {
        mods.push(me);
    }
}

/// The mod's altar zone filters, and how often it asked for a new sizing.
#[derive(Debug, Default)]
pub(crate) struct ZoneSizing {
    /// The filters and their pointers.
    pub(crate) filters: Vec<Registered<AltarZoneFilterFn>>,
    /// How often the mod asked for the zones to be sized again.
    pub(crate) resizes: usize,
}

entries! {
    pub(crate) fn request_altars(me as Mod) -> Status {
        crate::test::table::with_open(|s| {
            if s.host.world.no_altar_code {
                return Status::UNAVAILABLE;
            }
            opened(&mut s.world.altar_openers, me);
            Status::OK
        })
    }

    pub(crate) fn altars(
        me as Mod,
        [out: *mut AltarInfo, entry_size: u32, capacity: u32, count: *mut u32] as args::SizedListOut,
    ) -> Status {
        let Some(altars) = with(None, |s| {
            s.world
                .altar_openers
                .contains(&me)
                .then(|| s.world.altars.clone())
        }) else {
            return Status::REFUSED;
        };
        list(&altars, out, entry_size, capacity, count)
    }

    pub(crate) fn altar_zone_filter(
        me as Mod,
        callback: Option<AltarZoneFilterFn> as args::Callback,
        user: *mut c_void,
    ) -> Status {
        let Some(callback) = callback else {
            return Status::BAD_ARGS;
        };
        crate::test::table::with_open(|s| {
            // The filter opens the altars, as the loader's does.
            if s.host.world.no_altar_code {
                return Status::UNAVAILABLE;
            }
            opened(&mut s.world.altar_openers, me);
            opened(&mut s.world.zone_controllers, me);
            s.world
                .zone_sizing
                .filters
                .push(Registered::new(callback, user, me));
            Status::OK
        })
    }

    pub(crate) fn altar_zones_changed(me as Mod) -> Status {
        with(Status::MISSING, |s| {
            if !s.world.zone_controllers.contains(&me) {
                return Status::REFUSED;
            }
            s.world.zone_sizing.resizes += 1;
            Status::OK
        })
    }

    pub(crate) fn building_zone_filter(
        me as Mod,
        callback: Option<BuildingZoneFilterFn> as args::Callback,
        user: *mut c_void,
    ) -> Status {
        let Some(callback) = callback else {
            return Status::BAD_ARGS;
        };
        crate::test::table::with_open(|s| {
            s.world
                .zone_filters
                .push(Registered::new(callback, user, me));
            Status::OK
        })
    }

    pub(crate) fn altar_limit_filter(
        me as Mod,
        callback: Option<AltarLimitFilterFn> as args::Callback,
        user: *mut c_void,
    ) -> Status {
        let Some(callback) = callback else {
            return Status::BAD_ARGS;
        };
        with(Status::MISSING, |s| {
            s.world
                .altar_limits
                .push(Registered::new(callback, user, me));
            Status::OK
        })
    }
}

/// The altar zones and bases the fake's game groups by itself, as the
/// game's.
const VANILLA: (u32, u32) = (16, 16);
/// The most altar zones and bases a mod may ask for, as the loader's.
const MOST: (u32, u32) = (255, 179);

/// The zones and bases the fake's game groups, as the loader's: the game's
/// own, or the mod's largest request once its hooks are armed.
pub(crate) fn zones_and_bases() -> (u32, u32) {
    let most = most();
    with(None, |s| {
        s.world
            .base_limit
            .requested
            .filter(|_| s.world.base_limit.armed)
    })
    .map_or(VANILLA, |(zones, bases)| {
        let cap = with(None, |s| s.world.base_limit.cap).unwrap_or(u32::MAX);
        (zones, bases.min(most.1).min(cap))
    })
}

/// The most a mod may ask for, as the loader's: its own most, or 16 bases
/// once arming finds a build that keeps them.
fn most() -> (u32, u32) {
    let kept = with(false, |s| {
        s.world.base_limit.armed && s.host.world.base_code == BaseCode::ZonesOnly
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
        if s.world.base_limit.armed {
            return Status::TOO_LATE;
        }
        if s.host.world.base_code == BaseCode::Unknown {
            return Status::UNAVAILABLE;
        }
        let (old_zones, old_bases) = s.world.base_limit.requested.unwrap_or(VANILLA);
        s.world.base_limit.requested = Some((
            zones.max(old_zones).max(VANILLA.0),
            bases.max(old_bases).max(VANILLA.1),
        ));
        Status::OK
    })
}

entries! {
    pub(crate) fn base_limit(me as Caller, out: *mut BaseLimitInfo as args::HeadedOut) -> Status {
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

    pub(crate) fn request_base_limits(
        me as Mod,
        zones: u32,
        bases: u32,
    ) -> Status {
        request(zones, bases)
    }

    pub(crate) fn set_base_limit(me as Mod, bases: u32) -> Status {
        if bases < VANILLA.1 {
            return Status::BAD_ARGS;
        }
        with(Status::MISSING, |s| {
            s.world.base_limit.cap = Some(bases);
            Status::OK
        })
    }

    pub(crate) fn base_zones(
        me as Caller,
        zones: *mut u32 as args::Out,
        bases: *mut u32 as args::Out,
    ) -> Status {
        let Some((zone_count, base_count)) = with(None, |s| s.world.grouping) else {
            return Status::UNAVAILABLE;
        };
        let status = put(zones, zone_count);
        if !status.is_ok() {
            return status;
        }
        put(bases, base_count)
    }
}
