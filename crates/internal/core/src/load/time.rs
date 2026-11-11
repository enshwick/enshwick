//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::{Load, Off};
use crate::{Load, Off};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

#[enshwick_macros::forward(base)]
impl Load<'_> {
    /// The mod's [`StepClock`], moved on by the server's steps: in the
    /// dedicated server, and on a client while it hosts a game. It moves in
    /// jumps of [`Ticked::DEFAULT_EVERY`](crate::events::Ticked::DEFAULT_EVERY)
    /// and costs nothing in between. Every call returns the same clock.
    ///
    /// # Errors
    ///
    /// [`Off`] after init, when the step event can no longer be subscribed.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    pub fn step_clock(&mut self) -> Result<&'static StepClock, Off>;
}
