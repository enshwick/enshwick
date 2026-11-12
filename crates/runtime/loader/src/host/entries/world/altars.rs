//! Host functions for altars, their zones and the base limit.

use crate::host::memory::list;
use crate::host::memory::{put, put_headed};
use crate::host::{caller, caller_index, entry};
use crate::world::base_groups::limit::Limits;
use enshwick_core::abi as api_frame;
use enshwick_core::abi::{ModHandle, Status};

pub(crate) extern "system" fn altars_open(me: ModHandle) -> Status {
    entry("altars_open", || {
        let (loader, index) = match caller_index(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        crate::world::altars::open(loader, index)
    })
}

pub(crate) extern "system" fn altars(
    me: ModHandle,
    out: *mut api_frame::AltarInfo,
    entry_size: u32,
    capacity: u32,
    count: *mut u32,
) -> Status {
    entry("altars", || {
        let (_, index) = match caller_index(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        if !crate::world::altars::opened(index) {
            return Status::REFUSED;
        }
        let altars = crate::world::altars::list();
        // SAFETY: the ABI has the mod pass `out` null or writable for
        // `capacity` entries of `entry_size` bytes and `count` null or a
        // writable `u32`, untouched by the mod during the call.
        unsafe { list(&altars, out, entry_size, capacity, count) }
    })
}

pub(crate) extern "system" fn altar_zone_filter(
    me: ModHandle,
    callback: api_frame::AltarZoneFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("altar_zone_filter", || match caller_index(me) {
        Ok((loader, index)) => crate::engine::conflicts::note(
            "altar zone filter",
            index,
            crate::engine::conflicts::Mode::Chained,
            crate::world::altars::add_sizer(loader, index, callback, user),
        ),
        Err(status) => status,
    })
}

pub(crate) extern "system" fn altar_zones_changed(me: ModHandle) -> Status {
    entry("altar_zones_changed", || match caller_index(me) {
        Ok((_, index)) => crate::world::altars::resize(index),
        Err(status) => status,
    })
}

pub(crate) extern "system" fn building_zone_check_filter(
    me: ModHandle,
    callback: api_frame::BuildingZoneCheckFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("building_zone_check_filter", || {
        let (_, index) = match caller_index(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        crate::engine::conflicts::note(
            "altar zone check",
            index,
            crate::engine::conflicts::Mode::Chained,
            crate::world::altars::register(index, callback, user),
        )
    })
}

pub(crate) extern "system" fn altar_limit_filter(
    me: ModHandle,
    callback: api_frame::AltarLimitFilterFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("altar_limit_filter", || match caller_index(me) {
        Ok((loader, index)) => crate::engine::conflicts::note(
            "altar limit filter",
            index,
            crate::engine::conflicts::Mode::Chained,
            crate::world::bases::add_limit(loader, index, callback, user),
        ),
        Err(status) => status,
    })
}

pub(crate) extern "system" fn base_limit(
    me: ModHandle,
    out: *mut api_frame::BaseLimitInfo,
) -> Status {
    entry("base_limit", || {
        if let Err(status) = caller(me) {
            return status;
        }
        let now = crate::world::bases::base_limit();
        let (vanilla, most) = (Limits::VANILLA, crate::world::bases::most());
        // SAFETY: `HostApi::base_limit` has the mod pass `out` null or
        // readable for its header and writable for the size it names, with no
        // other thread touching it during the call.
        unsafe {
            put_headed(
                out,
                api_frame::BaseLimitInfo {
                    header: api_frame::Header::of::<api_frame::BaseLimitInfo>(),
                    zones: now.zones,
                    bases: now.bases,
                    vanilla_zones: vanilla.zones,
                    reserved: 0,
                    vanilla_bases: vanilla.bases,
                    max_zones: most.zones,
                    max_bases: most.bases,
                },
            )
        }
    })
}

pub(crate) extern "system" fn raise_base_limit(me: ModHandle) -> Status {
    entry("raise_base_limit", || match caller_index(me) {
        Ok((loader, _)) => crate::world::bases::request(loader, Limits::RAISED),
        Err(status) => status,
    })
}

pub(crate) extern "system" fn request_base_limits(me: ModHandle, zones: u32, bases: u32) -> Status {
    entry("request_base_limits", || match caller_index(me) {
        Ok((loader, _)) => crate::world::bases::request(loader, Limits { zones, bases }),
        Err(status) => status,
    })
}

pub(crate) extern "system" fn set_base_cap(me: ModHandle, bases: u32) -> Status {
    entry("set_base_cap", || match caller_index(me) {
        Ok((_, owner)) => crate::world::bases::cap(owner, bases),
        Err(status) => status,
    })
}

pub(crate) extern "system" fn base_zones(
    me: ModHandle,
    zones: *mut u32,
    bases: *mut u32,
) -> Status {
    entry("base_zones", || {
        if let Err(status) = caller(me) {
            return status;
        }
        let Some((zone_count, base_count)) = crate::world::bases::zones() else {
            return Status::MISSING;
        };
        // SAFETY: `HostApi::base_zones` has the mod pass `zones` null or
        // pointing at a writable, aligned `u32` that no other thread touches
        // during the call.
        let status = unsafe { put(zones, zone_count) };
        if !status.is_ok() {
            return status;
        }
        // SAFETY: `HostApi::base_zones` has the mod pass `bases` null or
        // pointing at a writable, aligned `u32` that no other thread touches
        // during the call.
        unsafe { put(bases, base_count) }
    })
}
