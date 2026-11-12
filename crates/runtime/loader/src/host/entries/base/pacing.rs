//! Host functions for the engine's pacing places: the loop and server waits
//! and the task workers.

use enshwick_core::abi as api_frame;
use enshwick_core::abi::{ModHandle, Status};

use crate::host::memory::put;
use crate::host::{calling, entry};

pub(crate) extern "system" fn loop_wait(
    me: ModHandle,
    callback: api_frame::LoopWaitFn,
    user: *mut core::ffi::c_void,
    found: *mut api_frame::LoopWaitSites,
) -> Status {
    entry("loop_wait", || {
        if found.is_null() {
            return Status::BAD_ARGS;
        }
        let (loader, index, _) = match calling(me) {
            Ok(called) => called,
            Err(status) => return status,
        };
        let sites = crate::engine::pacing::loop_wait(loader, index, callback, user);
        // SAFETY: `HostApi::loop_wait` has the mod pass `found` null or
        // pointing at a writable `LoopWaitSites` no other thread touches
        // during the call.
        unsafe { put(found, sites) }
    })
}

pub(crate) extern "system" fn server_wait(
    me: ModHandle,
    callback: api_frame::ServerWaitFn,
    user: *mut core::ffi::c_void,
    found: *mut u32,
) -> Status {
    entry("server_wait", || {
        if found.is_null() {
            return Status::BAD_ARGS;
        }
        let (loader, index, _) = match calling(me) {
            Ok(called) => called,
            Err(status) => return status,
        };
        let hooked = crate::engine::pacing::server_wait(loader, index, callback, user);
        // SAFETY: `HostApi::server_wait` has the mod pass `found` null or
        // pointing at a writable `u32` no other thread touches during the
        // call.
        unsafe { put(found, u32::from(hooked)) }
    })
}

pub(crate) extern "system" fn task_workers(
    me: ModHandle,
    default: u32,
    background: u32,
    found: *mut u32,
) -> Status {
    entry("task_workers", || {
        if found.is_null() {
            return Status::BAD_ARGS;
        }
        let (loader, index, _) = match calling(me) {
            Ok(called) => called,
            Err(status) => return status,
        };
        let capped = crate::engine::pacing::task_workers(loader, index, default, background);
        // SAFETY: `HostApi::task_workers` has the mod pass `found` null or
        // pointing at a writable `u32` no other thread touches during the
        // call.
        unsafe { put(found, u32::from(capped)) }
    })
}
