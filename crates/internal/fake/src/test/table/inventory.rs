//! The fake's inventory entries: requests, item filters, damage filters,
//! survivors and effects (game events, teleports, spawns and drops).

use core::ffi::c_void;

use enshwick_core::__fake::ids;

use crate::test::SubmittedRequest;
use crate::test::table::{Registered, Registration, with};

pub(crate) mod containers;

/// A submitted inventory request and where its result goes.
pub(crate) struct OpenRequest {
    pub(crate) request: SubmittedRequest,
    pub(crate) done: Registered<InventoryResultFn>,
}
use enshwick_core::abi::{
    BedCheckFn, DamageFilterFn, InventoryRequest, InventoryResultFn, ItemFilterFn, ModHandle, Side,
    SpawnRequest, Status, SummonFilterFn, SurvivorLookupFn, SurvivorStatusFn, TeleportRequest,
};

pub(crate) extern "system" fn item_filter(
    me: ModHandle,
    callback: Option<ItemFilterFn>,
    user: *mut c_void,
) -> Status {
    let Some(callback) = callback else {
        return Status::BAD_ARGS;
    };
    with(Status::MISSING, |s| {
        if s.registration == Registration::Closed {
            return Status::TOO_LATE;
        }
        if s.host.no_item_filters {
            return Status::UNAVAILABLE;
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
            return Some(Err(Status::MISSING));
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
    me: ModHandle,
    type_hash: u32,
    data: *const u8,
    size: u32,
) -> Status {
    if data.is_null() {
        return Status::BAD_ARGS;
    }
    if !crate::test::table::on_game_thread() {
        return Status::WRONG_THREAD;
    }
    // SAFETY: the safe layer passes `size` bytes at `data` for the call.
    let bytes = unsafe { core::slice::from_raw_parts(data, size as usize) }.to_vec();
    with(Status::MISSING, |s| {
        if s.edits.stopped {
            return Status::UNAVAILABLE;
        }
        let step = s.ticks.step;
        let notify = s.host.type_hashes.get(ids::NOTIFY_EVENT) == Some(&type_hash);
        if notify && !s.notified.has_room(me, step) {
            return Status::REFUSED;
        }
        if notify {
            s.notified.count(me, step);
        }
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
    if request.template == enshwick_core::content::Guid::NIL {
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
    callback: Option<DamageFilterFn>,
    user: *mut c_void,
) -> Status {
    let Some(callback) = callback else {
        return Status::BAD_ARGS;
    };
    with(Status::MISSING, |s| {
        if s.registration == Registration::Closed {
            return Status::TOO_LATE;
        }
        s.damage_filters.push(Registered::new(callback, user, me));
        Status::OK
    })
}

pub(crate) extern "system" fn survivor_lookup(
    me: ModHandle,
    callback: Option<SurvivorLookupFn>,
    user: *mut c_void,
) -> Status {
    let Some(callback) = callback else {
        return Status::BAD_ARGS;
    };
    with(Status::MISSING, |s| {
        if s.registration == Registration::Closed {
            return Status::TOO_LATE;
        }
        hold_each(&mut s.survivor_lookup, Registered::new(callback, user, me));
        Status::OK
    })
}

/// As the loader: each mod holds one registration, in load order, and a
/// second replaces its first.
fn hold_each<F: Copy>(held: &mut Vec<Registered<F>>, registered: Registered<F>) {
    match held.iter_mut().find(|r| r.owner() == registered.owner()) {
        Some(own) => *own = registered,
        None => held.push(registered),
    }
}

pub(crate) extern "system" fn survivor_bed_check(
    me: ModHandle,
    callback: Option<BedCheckFn>,
    user: *mut c_void,
) -> Status {
    let Some(callback) = callback else {
        return Status::BAD_ARGS;
    };
    with(Status::MISSING, |s| {
        if s.registration == Registration::Closed {
            return Status::TOO_LATE;
        }
        s.bed_checks.push(Registered::new(callback, user, me));
        Status::OK
    })
}

pub(crate) extern "system" fn summon_filter(
    me: ModHandle,
    callback: Option<SummonFilterFn>,
    user: *mut c_void,
) -> Status {
    let Some(callback) = callback else {
        return Status::BAD_ARGS;
    };
    with(Status::MISSING, |s| {
        if s.registration == Registration::Closed {
            return Status::TOO_LATE;
        }
        s.summons.push(Registered::new(callback, user, me));
        Status::OK
    })
}

pub(crate) extern "system" fn summoned(
    me: ModHandle,
    callback: Option<SummonFilterFn>,
    user: *mut c_void,
) -> Status {
    let Some(callback) = callback else {
        return Status::BAD_ARGS;
    };
    with(Status::MISSING, |s| {
        if s.registration == Registration::Closed {
            return Status::TOO_LATE;
        }
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
        if s.registration == Registration::Closed {
            return Status::TOO_LATE;
        }
        s.summon_items.push((item, survivor));
        Status::OK
    })
}

pub(crate) extern "system" fn survivor_status(
    me: ModHandle,
    callback: Option<SurvivorStatusFn>,
    user: *mut c_void,
) -> Status {
    let Some(callback) = callback else {
        return Status::BAD_ARGS;
    };
    with(Status::MISSING, |s| {
        if s.registration == Registration::Closed {
            return Status::TOO_LATE;
        }
        if s.host.side != Side::CLIENT {
            return Status::UNAVAILABLE;
        }
        hold_each(&mut s.survivor_status, Registered::new(callback, user, me));
        Status::OK
    })
}

pub(crate) extern "system" fn request_inventory(_me: ModHandle) -> Status {
    with(Status::MISSING, |s| {
        if s.registration == Registration::Closed {
            return Status::TOO_LATE;
        }
        if s.host.no_inventories {
            Status::UNAVAILABLE
        } else {
            Status::OK
        }
    })
}

pub(crate) extern "system" fn inventory_submit(
    me: ModHandle,
    request: *const InventoryRequest,
    done: Option<InventoryResultFn>,
    user: *mut c_void,
) -> Status {
    let Some(done) = done else {
        return Status::BAD_ARGS;
    };
    // The loader requires only the fields through `op_count`.
    let required = core::mem::offset_of!(InventoryRequest, op_count) + size_of::<u32>();
    // SAFETY: the safe layer passes null or its version of the request,
    // readable for the size its header states, for the call.
    let Some(request) = (unsafe { enshwick_core::abi::read_headed(request, required) }) else {
        return Status::BAD_ARGS;
    };
    with(Status::MISSING, |s| {
        if s.host.no_inventories || (s.host.side != Side::SERVER && !s.host.hosting) {
            return Status::UNAVAILABLE;
        }
        // SAFETY: the request holds `op_count` operations, read during the
        // call.
        let Some(ops) = (unsafe {
            enshwick_core::__fake::copy::list_in(request.ops, request.op_count as usize)
        }) else {
            return Status::BAD_ARGS;
        };
        if enshwick_core::__fake::rules::inventory_checks::refused(ops) {
            return Status::BAD_ARGS;
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

#[cfg(test)]
mod tests {
    use enshwick_core::abi::{
        Header, InventoryOp, InventoryRequest, InventoryResult, ModHandle, Side, Status,
    };

    use crate::test::FakeHost;

    unsafe extern "system" fn ignored(result: *const InventoryResult, _: *mut core::ffi::c_void) {
        let _read = crate::test::table::base::io::get_headed(result);
    }

    fn give(count: u32) -> InventoryOp {
        InventoryOp {
            kind: InventoryOp::GIVE,

            target: enshwick_core::Entity(7),

            item: enshwick_core::ItemId::new(3),
            count,
            ..InventoryOp::default()
        }
    }

    fn submit(ops: &[InventoryOp]) -> Status {
        let request = InventoryRequest {
            header: Header::of::<InventoryRequest>(),
            flags: 0,
            actor: enshwick_core::Entity(0),
            ops: ops.as_ptr(),
            op_count: u32::try_from(ops.len()).unwrap_or(u32::MAX),
            reserved: 0,
        };
        super::inventory_submit(
            ModHandle(1),
            &raw const request,
            Some(ignored),
            core::ptr::null_mut(),
        )
    }

    #[test]
    fn the_fake_refuses_the_inventory_requests_the_loader_refuses() {
        let _loader = FakeHost::new().install();
        let too_many = vec![give(1); enshwick_core::inventory::MAX_OPS + 1];
        assert_eq!(
            submit(&too_many),
            Status::BAD_ARGS,
            "more ops than a request holds"
        );
        assert_eq!(submit(&[give(0)]), Status::BAD_ARGS, "a give of nothing");
        assert_eq!(submit(&[give(1)]), Status::OK);
    }

    #[test]
    fn the_fake_refuses_inventory_requests_on_a_client_that_hosts_nothing() {
        let _loader = FakeHost::new().side(Side::CLIENT).install();
        assert_eq!(submit(&[give(1)]), Status::UNAVAILABLE);
    }

    #[test]
    fn the_fake_checks_the_side_before_the_ops_as_the_loader_does() {
        let _loader = FakeHost::new().side(Side::CLIENT).install();
        let request = InventoryRequest {
            header: Header::of::<InventoryRequest>(),
            flags: 0,
            actor: enshwick_core::Entity(0),
            ops: core::ptr::null(),
            op_count: 3,
            reserved: 0,
        };
        let status = super::inventory_submit(
            ModHandle(1),
            &raw const request,
            Some(ignored),
            core::ptr::null_mut(),
        );
        assert_eq!(status, Status::UNAVAILABLE);
    }
}
