//! The fake's pacing places: every place is on the build, the worker caps
//! only on the server, the message wait only on the client.

use core::ffi::c_void;

use crate::abi::{LoopWaitFn, LoopWaitSites, ModHandle, ServerWaitFn, Side, Status};
use crate::test::table::{put, with};

pub(crate) extern "system" fn loop_wait(
    _me: ModHandle,
    callback: LoopWaitFn,
    user: *mut c_void,
    found: *mut LoopWaitSites,
) -> Status {
    let side = with(Side::SERVER, |s| {
        s.loop_waits.push((callback, user.expose_provenance()));
        s.host.side
    });
    let sites = if side == Side::CLIENT {
        LoopWaitSites(LoopWaitSites::HANDLES.0 | LoopWaitSites::MESSAGES.0)
    } else {
        LoopWaitSites::HANDLES
    };
    put(found, sites)
}

pub(crate) extern "system" fn server_wait(
    _me: ModHandle,
    callback: ServerWaitFn,
    user: *mut c_void,
    found: *mut u32,
) -> Status {
    with((), |s| {
        s.server_waits.push((callback, user.expose_provenance()));
    });
    put(found, 1)
}

pub(crate) extern "system" fn task_workers(
    _me: ModHandle,
    default: u32,
    background: u32,
    found: *mut u32,
) -> Status {
    let server = with(false, |s| {
        let lowest = |old: u32, new: u32| match (old, new) {
            (0, new) => new,
            (old, 0) => old,
            (old, new) => old.min(new),
        };
        let (d, b) = s.task_workers.unwrap_or((0, 0));
        s.task_workers = Some((lowest(d, default), lowest(b, background)));
        s.host.side == Side::SERVER
    });
    put(found, u32::from(server))
}
