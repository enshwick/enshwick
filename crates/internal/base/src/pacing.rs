//! How the engine paces its loops and sizes its thread pools: the places a
//! mod changes to make the game use less CPU.
//!
//! - [`Load::on_loop_wait`](../struct.Load.html#method.on_loop_wait): the engine's event loop, about to wait on its handles.
//!   It turns the time left before its deadline into whole milliseconds,
//!   truncating, so with less than a millisecond left it waits 0 ms, again
//!   and again, until the deadline passes.
//! - [`Load::on_server_wait`](../struct.Load.html#method.on_server_wait): the server's game loop picking how long it waits
//!   between its passes.
//! - [`Load::cap_task_workers`](../struct.Load.html#method.cap_task_workers): the most worker threads each task group starts
//!   with, which the engine sizes from the logical CPUs it sees.
//!
//! The loader finds each place on the running build and owns its hook; a
//! place this build lacks is reported, never guessed.
//!
//! # Examples
//!
//! ```
//! # #[cfg(any())] fn in_enshwick() { // names items above this crate: enshwick's doctests compile it
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     let found = load.on_loop_wait(|wait| {
//!         if wait.ms == 0 && wait.left_ns > 0 {
//!             // Sleep `left_ns` here instead of letting the game poll.
//!         }
//!     })?;
//!     if !found.handles {
//!         enshwick::warn!("the event loop's wait was not found");
//!     }
//!     Ok(())
//! }
//! # }
//! ```

use core::ffi::c_void;

use crate::abi::{LoopWaitData, LoopWaitSites};
use crate::{BaseLoad, Off};

/// The server loop's wait with nobody online.
pub const SERVER_IDLE_WAIT_NS: u64 = 40_000_000;
/// The server loop's wait with players online.
pub const SERVER_ONLINE_WAIT_NS: u64 = 1_000_000;
/// The server loop's wait after a pass longer than 14 ms.
pub const SERVER_SLOW_WAIT_NS: u64 = 100_000;

/// A wait of the engine's event loop on its handles, about to happen.
///
/// # Examples
///
/// See [`Load::on_loop_wait`](../struct.Load.html#method.on_loop_wait), whose handler gets one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, crate::Script)]
#[non_exhaustive]
pub struct LoopWait {
    /// What the game is about to wait, in milliseconds: the time left,
    /// truncated.
    pub ms: u32,
    /// The time left before the loop's deadline, in nanoseconds.
    pub left_ns: i64,
}

/// Which of the event loop's waits [`Load::on_loop_wait`](../struct.Load.html#method.on_loop_wait) found and hooked.
///
/// # Examples
///
/// ```
/// # extern crate enshwick_base as enshwick;
/// let none = enshwick::pacing::LoopWaits::default();
/// assert!(!none.handles && !none.messages);
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, crate::Script)]
#[non_exhaustive]
pub struct LoopWaits {
    /// The wait on handles, on the server and the client.
    pub handles: bool,
    /// The client's wait on handles and window messages.
    pub messages: bool,
}

unsafe extern "system" fn run_loop_wait<F>(wait: *const LoopWaitData, user: *mut c_void)
where
    F: Fn(LoopWait),
{
    // SAFETY: the loader passes a valid wait for the call; `as_ref` turns
    // away null.
    let Some(wait) = (unsafe { wait.as_ref() }) else {
        return;
    };
    let call = LoopWait {
        ms: wait.ms,
        left_ns: wait.left_ns,
    };
    // SAFETY: `user` is the leaked `F` this function was registered with.
    let handler = unsafe { &*user.cast::<F>() };
    crate::host::guard("panic in a loop wait handler", || (), || handler(call));
}

unsafe extern "system" fn run_server_wait<F>(wait_ns: *mut u64, user: *mut c_void)
where
    F: Fn(&mut u64),
{
    // SAFETY: the loader passes the wait for the call, writable; `as_mut`
    // turns away null.
    let Some(wait_ns) = (unsafe { wait_ns.as_mut() }) else {
        return;
    };
    // SAFETY: `user` is the leaked `F` this function was registered with.
    let handler = unsafe { &*user.cast::<F>() };
    crate::host::guard("panic in a server wait handler", || (), || handler(wait_ns));
}

/// The most worker threads the task groups start with; 0 leaves a group
/// as the game sizes it.
///
/// # Examples
///
/// ```
/// # extern crate enshwick_base as enshwick;
/// let game_sized = enshwick::pacing::TaskWorkers::default();
/// assert_eq!((game_sized.default, game_sized.background), (0, 0));
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, crate::Script)]
#[non_exhaustive]
pub struct TaskWorkers {
    /// The default group, which also sizes the ECS task queue.
    pub default: u8,
    /// The background group: streaming, saves and other slow work.
    pub background: u8,
}

impl TaskWorkers {
    /// The same cap for both groups.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_base as enshwick;
    /// let caps = enshwick::pacing::TaskWorkers::at_most(4);
    /// assert_eq!((caps.default, caps.background), (4, 4));
    /// ```
    #[must_use]
    pub const fn at_most(workers: u8) -> Self {
        Self {
            default: workers,
            background: workers,
        }
    }
}

impl BaseLoad {
    /// As `Load::on_server_wait_script`.
    #[doc(hidden)]
    pub fn on_server_wait_script(
        &mut self,
        handler: impl Fn(u64) -> Option<u64> + Send + Sync + 'static,
    ) -> Result<bool, Off> {
        self.on_server_wait(move |wait_ns| {
            if let Some(changed) = handler(*wait_ns) {
                *wait_ns = changed;
            }
        })
    }

    /// As `Load::on_loop_wait`.
    #[doc(hidden)]
    pub fn on_loop_wait<F>(&mut self, handler: F) -> Result<LoopWaits, Off>
    where
        F: Fn(LoopWait) + Send + Sync + 'static,
    {
        let host = self.host();
        let mut sites = LoopWaitSites::NONE;
        crate::load::hand_over(
            handler,
            |user| (host.api().loop_wait)(host.handle(), run_loop_wait::<F>, user, &raw mut sites),
            "the loop wait handler",
        )?;
        Ok(LoopWaits {
            handles: sites.contains(LoopWaitSites::HANDLES),
            messages: sites.contains(LoopWaitSites::MESSAGES),
        })
    }

    /// As `Load::on_server_wait`.
    #[doc(hidden)]
    pub fn on_server_wait<F>(&mut self, handler: F) -> Result<bool, Off>
    where
        F: Fn(&mut u64) + Send + Sync + 'static,
    {
        let host = self.host();
        let mut found = 0;
        crate::load::hand_over(
            handler,
            |user| {
                (host.api().server_wait)(host.handle(), run_server_wait::<F>, user, &raw mut found)
            },
            "the server wait handler",
        )?;
        Ok(found != 0)
    }

    /// As `Load::cap_task_workers`.
    #[doc(hidden)]
    pub fn cap_task_workers(&mut self, caps: TaskWorkers) -> Result<bool, Off> {
        if caps == TaskWorkers::default() {
            return Ok(false);
        }
        let host = self.host();
        let mut found = 0;
        let status = (host.api().task_workers)(
            host.handle(),
            u32::from(caps.default),
            u32::from(caps.background),
            &raw mut found,
        );
        crate::load::ok(status, found != 0, || "the task worker caps".to_owned())
    }
}
