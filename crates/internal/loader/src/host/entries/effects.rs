//! Host functions that change a player's level and experience and drop items on the ground.

use enshwick::Entity;
use enshwick::abi::{DropRequest, Headed, ModHandle, Status};

use crate::host::memory::put;
use crate::host::{caller, caller_index, entry};

pub(crate) extern "system" fn progress_write(
    me: ModHandle,
    entity: Entity,
    what: u32,
    value: u32,
) -> Status {
    entry("progress_write", || {
        match caller_index(me).and_then(|(loader, index)| {
            crate::progress::write(loader, index, entity.0, what, value)
        }) {
            Ok(()) => Status::OK,
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn drop_items(
    me: ModHandle,
    request: *const DropRequest,
    out: *mut Entity,
) -> Status {
    entry("drop_items", || {
        // SAFETY: the caller passes null or a request with its header; an
        // all-zero request is valid.
        let Some(request) = (unsafe { enshwick::abi::read_headed(request, DropRequest::BORN) })
        else {
            return Status::BAD_ARGS;
        };
        // Refused before the drop: after it, a mod told BAD_ARGS would drop again.
        if out.is_null() {
            return Status::BAD_ARGS;
        }
        match caller(me).and_then(|loader| crate::drops::drop_items(loader, &request)) {
            // SAFETY: `out` is non-null (checked above) and `HostApi::drop_items`
            // has the mod pass a writable, aligned `Entity` that no other
            // thread touches during the call.
            Ok(entity) => unsafe { put(out, Entity(entity)) },
            Err(status) => status,
        }
    })
}
