//! The fake's inventory entries: requests, item filters, damage filters,
//! survivors and effects (game events, teleports, spawns and drops).

use core::ffi::c_void;

use crate::test::SubmittedRequest;
use crate::test::table::{Registered, Registration, with};

/// A submitted inventory request and where its result goes.
pub(crate) struct OpenRequest {
    pub(crate) request: SubmittedRequest,
    pub(crate) done: Registered<InventoryResultFn>,
}
use enshwick_core::abi::{
    BedCheckFn, DamageFilterFn, InventoryRequest, InventoryResultFn, ItemActionFn, ModHandle, Side,
    SpawnRequest, Status, SummonFn, SurvivorLookupFn, SurvivorStatusFn, TeleportRequest,
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
        if s.host.no_item_filters {
            return Status::MISSING;
        }
        s.item_filters.push(Registered::new(callback, user, me));
        Status::OK
    })
}

pub(crate) extern "system" fn drop_items(
    _me: ModHandle,
    request: *const enshwick_core::abi::DropRequest,
    out: *mut enshwick_core::Entity,
) -> Status {
    let Some(request) = crate::test::table::get_headed(request) else {
        return Status::BAD_ARGS;
    };
    if out.is_null() {
        return Status::BAD_ARGS;
    }
    let rarity = u8::try_from(request.rarity).ok();
    if !(1..=u32::from(u16::MAX)).contains(&request.count)
        || rarity
            .and_then(enshwick_core::affixes::ItemRarity::from_raw)
            .is_none()
    {
        return Status::BAD_ARGS;
    }
    let entity = with(None, |s| {
        if !s.host.items.contains_key(&request.item.get()) {
            return Some(Err(Status::REFUSED));
        }
        // Above every spawn's id, so the two never share one.
        let entity = crate::test::table::world::FIRST_SPAWNED
            + 0x1_0000
            + u32::try_from(s.drops.len()).unwrap_or(0);
        s.drops.push((enshwick_core::Entity(entity), request));
        Some(Ok(entity))
    });
    match entity {
        Some(Ok(entity)) => crate::test::table::put(out, enshwick_core::Entity(entity)),
        Some(Err(status)) => status,
        None => Status::MISSING,
    }
}

pub(crate) extern "system" fn emit(
    _me: ModHandle,
    type_hash: u32,
    data: *const u8,
    size: u32,
) -> Status {
    if data.is_null() {
        return Status::BAD_ARGS;
    }
    // SAFETY: the safe layer passes `size` bytes at `data` for the call.
    let bytes = unsafe { core::slice::from_raw_parts(data, size as usize) }.to_vec();
    with(Status::MISSING, |s| {
        s.emits.push((type_hash, bytes));
        Status::OK
    })
}

pub(crate) extern "system" fn teleport(_me: ModHandle, request: *const TeleportRequest) -> Status {
    let Some(request) = crate::test::table::get_headed(request) else {
        return Status::BAD_ARGS;
    };
    with(Status::MISSING, |s| {
        if crate::test::table::world::entity_calls_off(s) {
            return Status::UNAVAILABLE;
        }
        if !s.host.templates.contains_key(&request.entity.0)
            && !s
                .host
                .placed
                .iter()
                .any(|(_, p)| p.entity == request.entity)
        {
            return Status::MISSING;
        }
        s.teleports.push(request);
        Status::OK
    })
}

pub(crate) extern "system" fn spawn(
    _me: ModHandle,
    request: *const SpawnRequest,
    out: *mut enshwick_core::Entity,
) -> Status {
    let Some(request) = crate::test::table::get_headed(request) else {
        return Status::BAD_ARGS;
    };
    if request.template == [0; 16] {
        return Status::BAD_ARGS;
    }
    let entity = with(None, |s| {
        let entity =
            crate::test::table::world::FIRST_SPAWNED + u32::try_from(s.spawns.len()).unwrap_or(0);
        s.spawns.push((entity, request));
        s.spawn_steps.push(s.ticks.step);
        Some(entity)
    });
    match entity {
        Some(entity) => crate::test::table::put(out, enshwick_core::Entity(entity)),
        None => Status::MISSING,
    }
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
        if s.registration == Registration::Closed {
            return Status::TOO_LATE;
        }
        if s.host.side != Side::CLIENT {
            return Status::MISSING;
        }
        if s.survivor_status
            .is_some_and(|held| held.owner() != Some(me))
        {
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
