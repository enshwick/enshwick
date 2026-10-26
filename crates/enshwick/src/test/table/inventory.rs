//! The fake's inventory requests.

use core::ffi::c_void;

use crate::abi::{InventoryRequest, InventoryResultFn, ItemData, ModHandle, Status};
use crate::test::SubmittedRequest;
use crate::test::table::{OpenRequest, found, with};

/// The table every installed fake answers through; a mod keeps the first
pub(crate) extern "system" fn inventory_open(_me: ModHandle) -> Status {
    with(Status::MISSING, |s| {
        if s.host.no_inventories {
            Status::MISSING
        } else {
            Status::OK
        }
    })
}

pub(crate) extern "system" fn item_info(_me: ModHandle, item: u32, out: *mut ItemData) -> Status {
    found(with(None, |s| s.host.items.get(&item).copied()), out)
}

pub(crate) extern "system" fn inventory_submit(
    _me: ModHandle,
    request: *const InventoryRequest,
    done: InventoryResultFn,
    user: *mut c_void,
) -> Status {
    // SAFETY: the safe layer passes a valid request for the call; `as_ref`
    // turns away null.
    let Some(request) = (unsafe { request.as_ref() }) else {
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
                actor: request.actor,
            },
            done,
            user: user.expose_provenance(),
        });
        Status::OK
    })
}
