//! Host functions for inventory requests.

use enshwick::abi as api_frame;
use enshwick::abi::{ModHandle, Status};

use crate::host::memory::put_headed;
use crate::host::{caller, caller_index, entry};

pub(crate) extern "system" fn inventory_open(me: ModHandle) -> Status {
    entry("inventory_open", || match caller(me) {
        Ok(loader) => crate::inventory::open(loader),
        Err(status) => status,
    })
}

pub(crate) extern "system" fn item_info(
    me: ModHandle,
    item: u32,
    out: *mut api_frame::ItemData,
) -> Status {
    entry("item_info", || {
        match caller(me).and_then(|loader| crate::inventory::item_info(loader, item)) {
            // SAFETY: `HostApi::item_info` has the mod pass `out` null or
            // pointing at its version of `ItemData`, writable for the size its
            // header states, that no other thread touches during the call.
            Ok(data) => unsafe { put_headed(out, data) },
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn inventory_submit(
    me: ModHandle,
    request: *const api_frame::InventoryRequest,
    done: api_frame::InventoryResultFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("inventory_submit", || {
        let (loader, index) = match caller_index(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        let required = core::mem::offset_of!(api_frame::InventoryRequest, op_count) + 4;
        // SAFETY: a non-null request from a mod points at a valid request for the
        // call; an all-zero request is valid.
        let Some(request) = (unsafe { enshwick::abi::read_headed(request, required) }) else {
            return Status::BAD_ARGS;
        };
        crate::inventory::submit(loader, index, &request, done, user)
    })
}
