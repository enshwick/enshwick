//! Host functions for the engine's pacing places: the loop and server waits
//! and the task workers.

use enshwick_core::abi as api_frame;
use enshwick_core::abi::Status;

use crate::host::layer::{Mod, Out, entries};

entries! {
    fn loop_wait(
        me as Mod,
        callback: api_frame::LoopWaitFn,
        user: *mut core::ffi::c_void,
        found as Out<api_frame::LoopWaitSites>,
    ) -> Status {
        found.put(crate::engine::pacing::loop_wait(me.loader, me.index, callback, user))
    }

    fn server_wait(
        me as Mod,
        callback: api_frame::ServerWaitFn,
        user: *mut core::ffi::c_void,
        found as Out<u32>,
    ) -> Status {
        let hooked = crate::engine::pacing::server_wait(me.loader, me.index, callback, user);
        found.put(u32::from(hooked))
    }

    fn task_workers(me as Mod, default: u32, background: u32, found as Out<u32>) -> Status {
        let capped = crate::engine::pacing::task_workers(me.loader, me.index, default, background);
        found.put(u32::from(capped))
    }
}
