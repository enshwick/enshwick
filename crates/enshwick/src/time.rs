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

use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use crate::{Load, Off};

/// Time counted by the server's steps, readable from any thread.
///
/// Steps come on the game thread and readers may sit on workers, so the
/// clock keeps its time in atomics: [`step`](Self::step) and
/// [`now`](Self::now) never lock.
///
/// # Examples
///
/// ```
/// use std::time::{Duration, Instant};
///
/// use enshwick::time::StepClock;
///
/// let clock = StepClock::new();
/// let start = Instant::now();
/// clock.step(start);
/// clock.step(start + Duration::from_millis(16));
/// // A minute's stall counts as one long step.
/// clock.step(start + Duration::from_secs(60));
/// assert_eq!(clock.now(), Duration::from_millis(16) + StepClock::MAX_STEP);
/// ```
#[derive(Debug)]
pub struct StepClock {
    epoch: OnceLock<Instant>,
    /// When the last step ran, in nanoseconds after `epoch` plus one; 0
    /// before the first.
    last: AtomicU64,
    now: AtomicU64,
}

impl StepClock {
    /// The most one step counts for.
    pub const MAX_STEP: Duration = Duration::from_millis(250);

    /// A clock at zero that has seen no step.
    ///
    /// # Examples
    ///
    /// See [`StepClock`].
    #[must_use]
    pub const fn new() -> Self {
        Self {
            epoch: OnceLock::new(),
            last: AtomicU64::new(0),
            now: AtomicU64::new(0),
        }
    }

    /// Counts a step that ran at `at`: the time since the latest step
    /// before, up to [`MAX_STEP`](Self::MAX_STEP); a step earlier than that
    /// counts nothing. The first step only starts the clock. [`Load::step_clock`] calls it for the mod's clock.
    ///
    /// # Examples
    ///
    /// See [`StepClock`].
    pub fn step(&self, at: Instant) {
        let epoch = *self.epoch.get_or_init(|| at);
        let at = u64::try_from(at.saturating_duration_since(epoch).as_nanos())
            .unwrap_or(u64::MAX)
            .saturating_add(1);
        let last = self.last.fetch_max(at, Ordering::Relaxed);
        if last != 0 && at > last {
            let max = u64::try_from(Self::MAX_STEP.as_nanos()).unwrap_or(u64::MAX);
            self.now.fetch_add((at - last).min(max), Ordering::Relaxed);
        }
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
    /// The mod's [`StepClock`], moved on by every server step: in the
    /// dedicated server, and on a client while it hosts a game. Every call
    /// returns the same clock.
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
        self.on::<crate::events::Ticked>(|_| {
            if let Some(clock) = CLOCK.get() {
                clock.step(Instant::now());
            }
        })?;
        Ok(CLOCK.get_or_init(StepClock::new))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_step_starts_the_clock_and_later_ones_count() {
        let clock = StepClock::new();
        let t0 = Instant::now();
        clock.step(t0);
        assert_eq!(clock.now(), Duration::ZERO);
        clock.step(t0 + Duration::from_millis(16));
        assert_eq!(clock.now(), Duration::from_millis(16));
        clock.step(t0 + Duration::from_secs(60));
        assert_eq!(clock.now(), Duration::from_millis(16) + StepClock::MAX_STEP);
    }

    #[test]
    fn a_late_step_out_of_order_counts_no_time_twice() {
        let clock = StepClock::new();
        let t0 = Instant::now();
        for ms in [0, 100, 50, 116] {
            clock.step(t0 + Duration::from_millis(ms));
        }
        assert_eq!(clock.now(), Duration::from_millis(116));
    }

    #[test]
    fn a_step_out_of_order_counts_nothing() {
        let clock = StepClock::new();
        let t0 = Instant::now();
        clock.step(t0 + Duration::from_millis(100));
        clock.step(t0);
        assert_eq!(clock.now(), Duration::ZERO);
    }
}
