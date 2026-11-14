//! Host functions for altars, their zones and the base limit.

use enshwick_core::abi as api_frame;
use enshwick_core::abi::Status;

use crate::engine::conflicts::Mode;
use crate::host::layer::{Caller, HeadedOut, Mod, Out, SizedListOut, checked, entries};
use crate::world::base_groups::limit::Limits;

entries! {
    fn altars_open(me as Mod) -> Status {
        checked(crate::world::altars::open(me.loader, me.index))
    }

    fn altars(
        me as Mod,
        [out, entry_size, capacity, count] as SizedListOut<api_frame::AltarInfo>,
    ) -> Status {
        if !crate::world::altars::opened(me.index) {
            return Err(Status::REFUSED.into());
        }
        out.put(&crate::world::altars::list())
    }

    fn altar_zone_filter(
        me as Mod,
        callback: api_frame::AltarZoneFn,
        user: *mut core::ffi::c_void,
    ) -> Status {
        me.claim(
            "altar zone filter",
            Mode::Chained,
            checked(crate::world::altars::add_sizer(me.loader, me.index, callback, user)),
        )
    }

    fn altar_zones_changed(me as Mod) -> Status {
        checked(crate::world::altars::resize(me.index))
    }

    fn building_zone_check_filter(
        me as Mod,
        callback: api_frame::BuildingZoneCheckFn,
        user: *mut core::ffi::c_void,
    ) -> Status {
        me.claim(
            "altar zone check",
            Mode::Chained,
            checked(crate::world::altars::register(me.index, callback, user)),
        )
    }

    fn altar_limit_filter(
        me as Mod,
        callback: api_frame::AltarLimitFilterFn,
        user: *mut core::ffi::c_void,
    ) -> Status {
        me.claim(
            "altar limit filter",
            Mode::Chained,
            checked(crate::world::bases::add_limit(me.loader, me.index, callback, user)),
        )
    }

    fn base_limit(_loader as Caller, out as HeadedOut<api_frame::BaseLimitInfo>) -> Status {
        let now = crate::world::bases::base_limit();
        let (vanilla, most) = (Limits::VANILLA, crate::world::bases::most());
        out.put(api_frame::BaseLimitInfo {
            header: api_frame::Header::of::<api_frame::BaseLimitInfo>(),
            zones: now.zones,
            bases: now.bases,
            vanilla_zones: vanilla.zones,
            reserved: 0,
            vanilla_bases: vanilla.bases,
            max_zones: most.zones,
            max_bases: most.bases,
        })
    }

    fn raise_base_limit(me as Mod) -> Status {
        checked(crate::world::bases::request(me.loader, Limits::RAISED))
    }

    fn request_base_limits(me as Mod, zones: u32, bases: u32) -> Status {
        checked(crate::world::bases::request(me.loader, Limits { zones, bases }))
    }

    fn set_base_cap(me as Mod, bases: u32) -> Status {
        checked(crate::world::bases::cap(me.index, bases))
    }

    fn base_zones(_loader as Caller, zones as Out<u32>, bases as Out<u32>) -> Status {
        let (zone_count, base_count) = crate::world::bases::zones().ok_or(Status::MISSING)?;
        zones.put(zone_count)?;
        bases.put(base_count)
    }
}
