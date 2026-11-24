//! The fake's pacing places, as the loader answers: every place is on the
//! build, the worker caps take only on the server, the message wait is only
//! on the client, and a second handler of the mod at a place links nothing.

use crate::test::table::base::args;
use crate::test::table::entries;
use core::ffi::c_void;

use crate::test::table::{Registered, put, with};
use enshwick_core::abi::{LoopWaitFn, LoopWaitSites, ServerWaitFn, Side, Status};

/// What the mod asked of the engine's pacing places.
#[derive(Debug, Default)]
pub(crate) struct Pacing {
    /// The loop wait handlers and their pointers.
    pub(crate) loop_waits: Vec<Registered<LoopWaitFn>>,
    /// The server wait handlers and their pointers.
    pub(crate) server_waits: Vec<Registered<ServerWaitFn>>,
    /// The lowest worker caps asked for, default and background.
    pub(crate) task_workers: Option<(u32, u32)>,
}

entries! {
    pub(crate) fn loop_wait(
        me as Mod,
        callback: Option<LoopWaitFn> as args::Callback,
        user: *mut c_void,
        found: *mut LoopWaitSites as args::Out,
    ) -> Status {
        let Some(callback) = callback else {
            return Status::BAD_ARGS;
        };
        if crate::test::table::late() {
            return Status::TOO_LATE;
        }
        let (side, first) = with((Side::SERVER, false), |s| {
            let first = s.base.pacing.loop_waits.is_empty();
            if first {
                s.base
                    .pacing
                    .loop_waits
                    .push(Registered::new(callback, user, me));
            }
            (s.host.base.side, first)
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

    pub(crate) fn server_wait(
        me as Mod,
        callback: Option<ServerWaitFn> as args::Callback,
        user: *mut c_void,
        found: *mut u8 as args::Out,
    ) -> Status {
        let Some(callback) = callback else {
            return Status::BAD_ARGS;
        };
        if crate::test::table::late() {
            return Status::TOO_LATE;
        }
        let first = with(false, |s| {
            let first = s.base.pacing.server_waits.is_empty();
            if first {
                s.base
                    .pacing
                    .server_waits
                    .push(Registered::new(callback, user, me));
            }
            first
        });
        put(found, u8::from(first))
    }

    pub(crate) fn task_workers(
        me as Mod,
        default: u32,
        background: u32,
        found: *mut u8 as args::Out,
    ) -> Status {
        if crate::test::table::late() {
            return Status::TOO_LATE;
        }
        let server = with(false, |s| {
            if s.host.base.side != Side::SERVER || (default, background) == (0, 0) {
                return false;
            }
            let lowest = |old: u32, new: u32| match (old, new) {
                (0, new) => new,
                (old, 0) => old,
                (old, new) => old.min(new),
            };
            let (d, b) = s.base.pacing.task_workers.unwrap_or((0, 0));
            s.base.pacing.task_workers = Some((lowest(d, default), lowest(b, background)));
            true
        });
        put(found, u8::from(server))
    }
}
