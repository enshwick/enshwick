//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::pacing::{LoopWait, LoopWaits, TaskWorkers};
use crate::{Load, Off};

#[enshwick_macros::forward(base)]
#[crate::script(pacing)]
impl Load<'_> {
    /// Runs `handler` where the server's game loop has picked its wait, as
    /// [`Load::on_server_wait`] does: it returns the wait in nanoseconds,
    /// or nil to keep it.
    ///
    /// # Errors
    ///
    /// As [`Load::on_server_wait`].
    #[script(name = "on_server_wait")]
    pub(crate) fn on_server_wait_script(
        &mut self,
        handler: impl Fn(u64) -> Option<u64> + Send + Sync + 'static,
    ) -> Result<bool, Off>;

    /// Runs `handler` before every wait of the engine's event loop on its
    /// handles, on the thread that waits: the app's main loop, the server's
    /// game loop and its session, and on the client its message wait too. A
    /// handler that sleeps must never sleep past `left_ns`, and must leave the
    /// waiting to the game, so it sees every event. Allowed during
    /// [`Mod::init`](crate::Mod::init) only.
    ///
    /// # Errors
    ///
    /// [`Off`] when the loader refuses a hook. A wait this build lacks is
    /// reported as not found in the result.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let found = load.on_loop_wait(|wait| {
    ///         let _ = (wait.ms, wait.left_ns);
    ///     })?;
    ///     let _ = found.messages;
    ///     Ok(())
    /// }
    /// ```
    #[script]
    pub fn on_loop_wait<F>(&mut self, handler: F) -> Result<LoopWaits, Off>
    where
        F: Fn(LoopWait) + Send + Sync + 'static;

    /// Runs `handler` where the server's game loop has picked its wait between
    /// two passes, in nanoseconds: [`SERVER_IDLE_WAIT_NS`](crate::pacing::SERVER_IDLE_WAIT_NS),
    /// [`SERVER_ONLINE_WAIT_NS`](crate::pacing::SERVER_ONLINE_WAIT_NS) or [`SERVER_SLOW_WAIT_NS`](crate::pacing::SERVER_SLOW_WAIT_NS). The handler may
    /// change it; the loop waits on its handles for that long, so events still
    /// end the wait early. On the server's game thread, once a pass; `false`
    /// when this build lacks the site. Allowed during
    /// [`Mod::init`](crate::Mod::init) only.
    ///
    /// # Errors
    ///
    /// [`Off`] when the loader refuses the hook.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::pacing;
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let hooked = load.on_server_wait(|wait_ns| {
    ///         if *wait_ns == pacing::SERVER_ONLINE_WAIT_NS {
    ///             *wait_ns = 2 * pacing::SERVER_ONLINE_WAIT_NS;
    ///         }
    ///     })?;
    ///     let _ = hooked;
    ///     Ok(())
    /// }
    /// ```
    pub fn on_server_wait<F>(&mut self, handler: F) -> Result<bool, Off>
    where
        F: Fn(&mut u64) + Send + Sync + 'static;

    /// Caps the worker threads of the task groups as the task system starts.
    /// The engine sizes them from the logical CPUs it sees, which under Wine
    /// are the host's, whatever a container allows. Only the dedicated server
    /// starts its task system after the loader arms hooks; on the client it has
    /// started already and the result is `false`, as it is when this build lacks
    /// the site or both caps are 0. The game's log shows the counts it started with:
    /// `[task] Group 'Default' Worker:…`. Allowed during
    /// [`Mod::init`](crate::Mod::init) only.
    ///
    /// # Errors
    ///
    /// [`Off`] when the loader refuses a hook.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::pacing::TaskWorkers;
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     if !load.cap_task_workers(TaskWorkers::at_most(4))? {
    ///         enshwick::info!("task workers stay as the game sizes them");
    ///     }
    ///     Ok(())
    /// }
    /// ```
    #[script]
    pub fn cap_task_workers(&mut self, caps: TaskWorkers) -> Result<bool, Off>;
}
