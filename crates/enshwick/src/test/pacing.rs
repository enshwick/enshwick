//! The engine's pacing places, as the loader would run the mod's handlers.

use super::Installed;
use super::table;
use crate::abi::{Header, LoopWaitData};

#[expect(
    clippy::unused_self,
    reason = "the fake's state is process-wide; `self` proves it is installed"
)]
impl Installed {
    /// Runs the mod's loop wait handlers on a wait of `ms` milliseconds with
    /// `left_ns` nanoseconds left before the loop's deadline.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// loader.loop_wait(0, 400_000);
    /// ```
    pub fn loop_wait(&self, ms: u32, left_ns: i64) {
        let handlers = table::with(Vec::new(), |s| s.loop_waits.clone());
        let wait = LoopWaitData {
            header: Header::of::<LoopWaitData>(),
            ms,
            reserved: 0,
            left_ns,
        };
        for (callback, user) in handlers {
            // SAFETY: a valid registration; `user` is the mod's pointer.
            unsafe {
                callback(
                    &raw const wait,
                    core::ptr::with_exposed_provenance_mut(user),
                );
            }
        }
    }

    /// The server loop's wait after the mod's handlers saw `wait_ns`, in load
    /// order.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert_eq!(loader.server_wait(1_000_000), 1_000_000);
    /// ```
    #[must_use]
    pub fn server_wait(&self, mut wait_ns: u64) -> u64 {
        let handlers = table::with(Vec::new(), |s| s.server_waits.clone());
        for (callback, user) in handlers {
            // SAFETY: a valid registration; `user` is the mod's pointer.
            unsafe {
                callback(
                    &raw mut wait_ns,
                    core::ptr::with_exposed_provenance_mut(user),
                );
            }
        }
        wait_ns
    }

    /// The worker caps the mods asked for, the default group's then the
    /// background group's, 0 for one left alone; `None` when no mod asked.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert_eq!(loader.task_workers(), None);
    /// ```
    #[must_use]
    pub fn task_workers(&self) -> Option<(u32, u32)> {
        table::with(None, |s| s.task_workers)
    }
}
