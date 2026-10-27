//! Time that runs with the server's simulation steps, for waits that should
//! not run out while the server stands still.
//!
//! A [`StepClock`] counts each step at its real length, up to
//! [`StepClock::MAX_STEP`]. A server that stalls, loads a world or hangs in
//! a debugger takes no steps, so the clock stands still with it, and the
//! first step after counts [`StepClock::MAX_STEP`] at most: a hold of ten
//! seconds is about ten seconds of the world running, not of the wall
//! clock.
//! [`Load::step_clock`] hands out the mod's clock, which the loader's
//! [`Ticked`](crate::events::Ticked) event moves on.
//!
//! # Examples
//!
//! ```
//! use std::time::Duration;
//!
//! use enshwick::time::StepClock;
//!
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     let clock = load.step_clock()?;
//!     let started = clock.now();
//!     load.on::<enshwick::events::Ticked>(move |_| {
//!         if clock.now().saturating_sub(started) > Duration::from_secs(10) {
//!             enshwick::info!("ten seconds of world time");
//!         }
//!     })
//! }
//! ```

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use crate::{Load, Off};

/// Time counted by the server's steps, readable from any thread.
///
/// Steps come on the game thread and readers may sit on workers, so the
/// clock keeps its time in an atomic: [`advance`](Self::advance) and
/// [`now`](Self::now) never lock.
///
/// # Examples
///
/// ```
/// use std::time::Duration;
///
/// use enshwick::time::StepClock;
///
/// let clock = StepClock::new();
/// clock.advance(Duration::from_millis(16), 1);
/// // A minute's stall in one step counts as one long step.
/// clock.advance(Duration::from_secs(60), 1);
/// assert_eq!(clock.now(), Duration::from_millis(16) + StepClock::MAX_STEP);
/// ```
#[derive(Debug)]
pub struct StepClock {
    now: AtomicU64,
}

impl StepClock {
    /// The most one step counts for: a tick that covers `n` steps counts
    /// `n` times this at most.
    pub const MAX_STEP: Duration = Duration::from_millis(250);

    /// A clock at zero.
    ///
    /// # Examples
    ///
    /// See [`StepClock`].
    #[must_use]
    pub const fn new() -> Self {
        Self {
            now: AtomicU64::new(0),
        }
    }

    /// Counts `steps` server steps that took `elapsed` together, up to
    /// [`MAX_STEP`](Self::MAX_STEP) for each: what a
    /// [`TickData`](crate::abi::TickData) says passed. [`Load::step_clock`]
    /// calls it for the mod's clock.
    ///
    /// # Examples
    ///
    /// See [`StepClock`].
    pub fn advance(&self, elapsed: Duration, steps: u64) {
        let max = crate::events::nanos(Self::MAX_STEP).saturating_mul(steps);
        let counted = crate::events::nanos(elapsed).min(max);
        self.now.fetch_add(counted, Ordering::Relaxed);
    }

    /// The time counted so far.
    ///
    /// # Examples
    ///
    /// See [`StepClock`].
    #[must_use]
    pub fn now(&self) -> Duration {
        Duration::from_nanos(self.now.load(Ordering::Relaxed))
    }
}

impl Default for StepClock {
    fn default() -> Self {
        Self::new()
    }
}

/// The mod's clock, moved on by its [`Ticked`](crate::events::Ticked)
/// subscription once made.
static CLOCK: crate::fresh::Fresh<StepClock> = crate::fresh::Fresh::new();

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
    pub fn step_clock(&mut self) -> Result<&'static StepClock, Off> {
        if let Some(clock) = CLOCK.get() {
            return Ok(clock);
        }
        self.on::<crate::events::Ticked>(|tick| {
            if let Some(clock) = CLOCK.get() {
                clock.advance(tick.elapsed(), tick.elapsed_steps());
            }
        })?;
        Ok(CLOCK.get_or_init(StepClock::new))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_step_counts_its_time_up_to_the_most_a_step_counts() {
        let clock = StepClock::new();
        clock.advance(Duration::from_millis(16), 1);
        assert_eq!(clock.now(), Duration::from_millis(16));
        clock.advance(Duration::from_secs(60), 1);
        assert_eq!(clock.now(), Duration::from_millis(16) + StepClock::MAX_STEP);
    }

    #[test]
    fn several_steps_together_may_count_the_most_of_each() {
        let clock = StepClock::new();
        clock.advance(Duration::from_millis(400), 2);
        assert_eq!(clock.now(), Duration::from_millis(400));
        clock.advance(Duration::from_secs(60), 3);
        assert_eq!(
            clock.now(),
            Duration::from_millis(400) + StepClock::MAX_STEP * 3
        );
    }

    #[test]
    fn a_tick_without_steps_counts_nothing() {
        let clock = StepClock::new();
        clock.advance(Duration::from_millis(50), 0);
        assert_eq!(clock.now(), Duration::ZERO);
    }
}
