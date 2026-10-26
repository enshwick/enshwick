//! The fake's pacing places, as the loader answers: every place is on the
//! build, the worker caps take only on the server, the message wait is only
//! on the client, and a second handler of the mod at a place links nothing.

use core::ffi::c_void;

use crate::abi::{LoopWaitFn, LoopWaitSites, ModHandle, ServerWaitFn, Side, Status};
use crate::test::table::{put, with};

pub(crate) extern "system" fn loop_wait(
    _me: ModHandle,
    callback: LoopWaitFn,
    user: *mut c_void,
    found: *mut LoopWaitSites,
) -> Status {
    let (side, first) = with((Side::SERVER, false), |s| {
        let first = s.pacing.loop_waits.is_empty();
        if first {
            s.pacing
                .loop_waits
                .push((callback, user.expose_provenance()));
        }
        (s.host.side, first)
    });
    let sites = if !first {
        LoopWaitSites::NONE
    } else if side == Side::CLIENT {
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
    let first = with(false, |s| {
        let first = s.pacing.server_waits.is_empty();
        if first {
            s.pacing
                .server_waits
                .push((callback, user.expose_provenance()));
        }
        first
    });
    put(found, u32::from(first))
}

pub(crate) extern "system" fn task_workers(
    _me: ModHandle,
    default: u32,
    background: u32,
    found: *mut u32,
) -> Status {
    let server = with(false, |s| {
        if s.host.side != Side::SERVER || (default, background) == (0, 0) {
            return false;
        }
        let lowest = |old: u32, new: u32| match (old, new) {
            (0, new) => new,
            (old, 0) => old,
            (old, new) => old.min(new),
        };
        let (d, b) = s.pacing.task_workers.unwrap_or((0, 0));
        s.pacing.task_workers = Some((lowest(d, default), lowest(b, background)));
        true
    });
    put(found, u32::from(server))
}
