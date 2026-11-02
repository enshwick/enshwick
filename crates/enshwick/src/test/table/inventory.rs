//! The fake's inventory requests.

use core::ffi::c_void;

use crate::abi::{InventoryRequest, InventoryResultFn, ItemData, ModHandle, Status};
use crate::test::SubmittedRequest;
use crate::test::table::{OpenRequest, with};

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
    item: crate::ItemId,
    out: *mut ItemData,
) -> Status {
    super::found_headed(with(None, |s| s.host.items.get(&item.get()).copied()), out)
}

pub(crate) extern "system" fn inventory_submit(
    _me: ModHandle,
    request: *const InventoryRequest,
    done: InventoryResultFn,
    user: *mut c_void,
) -> Status {
    // The loader requires only the fields through `op_count`.
    let required = core::mem::offset_of!(InventoryRequest, op_count) + size_of::<u32>();
    // SAFETY: the safe layer passes null or its version of the request,
    // readable for the size its header states, for the call.
    let Some(request) = (unsafe { crate::abi::read_headed(request, required) }) else {
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
            done,
            user: user.expose_provenance(),
        });
        Status::OK
    })
}
