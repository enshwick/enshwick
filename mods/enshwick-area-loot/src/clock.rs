//! The mod's time: it runs with the server's simulation steps, each counted
//! at its real length up to [`MAX_STEP`]. A stalled server, whose steps stop,
//! holds every wait still, so a take's hold or another player's busy window
//! never runs out while nothing could happen.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// The most one step counts for.
const MAX_STEP: Duration = Duration::from_millis(250);

/// Time counted by steps, as nanoseconds in atomics: the steps write it on
/// the game thread, the loot pass reads it on a worker.
#[derive(Debug)]
pub(crate) struct Clock {
    epoch: OnceLock<Instant>,
    /// When the last step ran, in nanoseconds after `epoch` plus one; 0
    /// before the first.
    last: AtomicU64,
    now: AtomicU64,
}

impl Clock {
    pub(crate) const fn new() -> Self {
        Self {
            epoch: OnceLock::new(),
            last: AtomicU64::new(0),
            now: AtomicU64::new(0),
        }
    }

    /// Counts a step that ran at `at`.
    pub(crate) fn step(&self, at: Instant) {
        let epoch = *self.epoch.get_or_init(|| at);
        let at = u64::try_from(at.saturating_duration_since(epoch).as_nanos())
            .unwrap_or(u64::MAX)
            .saturating_add(1);
        let last = self.last.swap(at, Ordering::Relaxed);
        if last != 0 {
            let max = u64::try_from(MAX_STEP.as_nanos()).unwrap_or(u64::MAX);
            let length = at.saturating_sub(last).min(max);
            self.now.fetch_add(length, Ordering::Relaxed);
        }
    }

    /// The time counted so far.
    pub(crate) fn now(&self) -> Duration {
        Duration::from_nanos(self.now.load(Ordering::Relaxed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_count_their_length_and_a_stall_counts_little() {
        let clock = Clock::new();
        let t0 = Instant::now();
        clock.step(t0);
        assert_eq!(clock.now(), Duration::ZERO);
        clock.step(t0 + Duration::from_millis(16));
        assert_eq!(clock.now(), Duration::from_millis(16));
        // A minute's stall between two steps.
        clock.step(t0 + Duration::from_secs(60));
        assert_eq!(clock.now(), Duration::from_millis(16) + MAX_STEP);
    }
}
