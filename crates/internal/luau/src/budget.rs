//! The script time a mod spends in each second, which its per-second budget
//! bounds.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

const WINDOW: Duration = Duration::from_secs(1);

/// The script time of one mod in the current one-second window, in
/// nanoseconds; a new window starts with the first call that ends a second
/// or more after the last window began.
#[derive(Debug, Default)]
pub(crate) struct Window {
    start: AtomicU64,
    spent: AtomicU64,
}

impl Window {
    /// Adds a call that took `took` and ended at `now`, a time since a fixed
    /// start; the script time of the window `now` falls in.
    pub(crate) fn charge(&self, now: Duration, took: Duration) -> Duration {
        let now = nanos(now);
        let took = nanos(took);
        let start = self.start.load(Ordering::Relaxed);
        let spent = if now.saturating_sub(start) >= nanos(WINDOW) {
            self.start.store(now, Ordering::Relaxed);
            self.spent.store(took, Ordering::Relaxed);
            took
        } else {
            self.spent
                .fetch_add(took, Ordering::Relaxed)
                .saturating_add(took)
        };
        Duration::from_nanos(spent)
    }
}

fn nanos(time: Duration) -> u64 {
    u64::try_from(time.as_nanos()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: Duration = Duration::from_millis(1);

    #[test]
    fn calls_in_one_second_add_up_and_a_new_second_starts_over() {
        let window = Window::default();
        let start = Duration::from_secs(5);
        assert_eq!(window.charge(start, 4 * MS), 4 * MS);
        assert_eq!(window.charge(start + 300 * MS, 4 * MS), 8 * MS);
        assert_eq!(window.charge(start + 999 * MS, 2 * MS), 10 * MS);
        assert_eq!(window.charge(start + 1000 * MS, 3 * MS), 3 * MS);
        assert_eq!(window.charge(start + 1500 * MS, 3 * MS), 6 * MS);
    }

    #[test]
    fn the_first_call_opens_a_window() {
        let window = Window::default();
        assert_eq!(window.charge(Duration::from_millis(10), 7 * MS), 7 * MS);
        assert_eq!(window.charge(Duration::from_millis(20), 7 * MS), 14 * MS);
    }
}
