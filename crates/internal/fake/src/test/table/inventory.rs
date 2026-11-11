//! The fake's inventory entries: requests, item filters, damage filters
//! and survivors.

use core::ffi::c_void;

use crate::test::SubmittedRequest;
use crate::test::table::{OpenRequest, Registered, Registration, with};
use enshwick_core::abi::{
    BedCheckFn, DamageFilterFn, InventoryRequest, InventoryResultFn, ItemActionFn, ItemData,
    ModHandle, Side, Status, SummonFn, SurvivorLookupFn, SurvivorStatusFn,
};

pub(crate) extern "system" fn item_filter(
    me: ModHandle,
    callback: ItemActionFn,
    user: *mut c_void,
) -> Status {
    with(Status::MISSING, |s| {
        if s.registration == Registration::Closed {
            return Status::TOO_LATE;
        }
        s.item_filters.push(Registered::new(callback, user, me));
        Status::OK
    })
}

pub(crate) extern "system" fn damage_filter(
    me: ModHandle,
    callback: DamageFilterFn,
    user: *mut c_void,
) -> Status {
    with(Status::MISSING, |s| {
        s.damage_filters.push(Registered::new(callback, user, me));
        Status::OK
    })
}

pub(crate) extern "system" fn survivor_lookup(
    me: ModHandle,
    callback: SurvivorLookupFn,
    user: *mut c_void,
) -> Status {
    with(Status::MISSING, |s| {
        if s.survivor_lookup.is_some() {
            return Status::REFUSED;
        }
        s.survivor_lookup = Some(Registered::new(callback, user, me));
        Status::OK
    })
}

pub(crate) extern "system" fn survivor_bed_check(
    me: ModHandle,
    callback: BedCheckFn,
    user: *mut c_void,
) -> Status {
    with(Status::MISSING, |s| {
        s.bed_checks.push(Registered::new(callback, user, me));
        Status::OK
    })
}

pub(crate) extern "system" fn summon_filter(
    me: ModHandle,
    callback: SummonFn,
    user: *mut c_void,
) -> Status {
    with(Status::MISSING, |s| {
        s.summons.push(Registered::new(callback, user, me));
        Status::OK
    })
}

pub(crate) extern "system" fn summoned(
    me: ModHandle,
    callback: SummonFn,
    user: *mut c_void,
) -> Status {
    with(Status::MISSING, |s| {
        s.summoned.push(Registered::new(callback, user, me));
        Status::OK
    })
}

pub(crate) extern "system" fn summon_item(
    _me: ModHandle,
    item: enshwick_core::ItemId,
    survivor: enshwick_core::survivors::NpcId,
) -> Status {
    with(Status::MISSING, |s| {
        s.summon_items.push((item, survivor));
        Status::OK
    })
}

pub(crate) extern "system" fn survivor_status(
    me: ModHandle,
    callback: SurvivorStatusFn,
    user: *mut c_void,
) -> Status {
    with(Status::MISSING, |s| {
        if s.host.side != Side::CLIENT || s.survivor_status.is_some() {
            return Status::REFUSED;
        }
        s.survivor_status = Some(Registered::new(callback, user, me));
        Status::OK
    })
}

pub(crate) extern "system" fn inventory_open(_me: ModHandle) -> Status {
    with(Status::MISSING, |s| {
        if s.host.no_inventories {
            Status::MISSING
        } else {
            Status::OK
        }
    })
}

pub(crate) extern "system" fn item_info(
    _me: ModHandle,
    item: enshwick_core::ItemId,
    out: *mut ItemData,
) -> Status {
    crate::test::table::found_headed(with(None, |s| s.host.items.get(&item.get()).copied()), out)
}

pub(crate) extern "system" fn inventory_submit(
    me: ModHandle,
    request: *const InventoryRequest,
    done: InventoryResultFn,
    user: *mut c_void,
) -> Status {
    // The loader requires only the fields through `op_count`.
    let required = core::mem::offset_of!(InventoryRequest, op_count) + size_of::<u32>();
    // SAFETY: the safe layer passes null or its version of the request,
    // readable for the size its header states, for the call.
    let Some(request) = (unsafe { enshwick_core::abi::read_headed(request, required) }) else {
        return Status::BAD_ARGS;
    };
    if request.ops.is_null() || request.op_count == 0 {
        return Status::BAD_ARGS;
    }
    // SAFETY: the request holds `op_count` operations, read during the call.
    let ops = unsafe { core::slice::from_raw_parts(request.ops, request.op_count as usize) };
    with(Status::MISSING, |s| {
        if s.host.no_inventories {
            return Status::MISSING;
        }
        s.inventory_requests.push(OpenRequest {
            request: SubmittedRequest {
                ops: ops.to_vec(),
                flags: request.flags,
                actor: request.actor.0,
            },
            done: Registered::new(done, user, me),
        });
        Status::OK
    })
}
