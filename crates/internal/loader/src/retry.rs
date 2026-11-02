//! Code patches a thread kept out at arming, tried again from the game step:
//! at the first step after arming, which follows the world's load, then up to
//! three more times a minute apart. Their owners wait meanwhile, with their
//! hooks bypassed; after the last try they are turned off.

use std::time::{Duration, Instant};

#[cfg(windows)]
mod run;

#[cfg(windows)]
pub(crate) use run::{Retry, after_step, hold};

/// Tries: the one at the first step and three more.
const TRIES: u8 = 4;
/// From a missed try to the next.
const TRY_EVERY: Duration = Duration::from_mins(1);
/// Checks in one try. A check that finds a thread inside waits for a step
/// [`CHECK_EVERY`] later, so a busy patch suspends the game's threads at
/// most once a second.
const CHECKS: u8 = 10;
/// From a check that found a thread inside to the next.
const CHECK_EVERY: Duration = Duration::from_secs(1);

/// When the waiting patches are checked, on the caller's clock.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Schedule {
    /// Tries left, the running one among them.
    tries: u8,
    /// Checks left in the running try.
    checks: u8,
    /// The next check; `None` after the last.
    next: Option<Instant>,
}

impl Schedule {
    /// A schedule whose first check is due at `start`.
    pub(crate) const fn new(start: Instant) -> Self {
        Self {
            tries: TRIES,
            checks: CHECKS,
            next: Some(start),
        }
    }

    /// The next check; `None` after the last.
    pub(crate) const fn next(&self) -> Option<Instant> {
        self.next
    }

    /// Whether a check is due at `now`.
    pub(crate) fn due(&self, now: Instant) -> bool {
        self.next.is_some_and(|at| now >= at)
    }

    /// Counts a check at `now` that found a thread inside; true when it was
    /// the last try's last check.
    pub(crate) fn missed(&mut self, now: Instant) -> bool {
        self.checks = self.checks.saturating_sub(1);
        if self.checks > 0 {
            self.next = Some(now + CHECK_EVERY);
            return false;
        }
        self.tries = self.tries.saturating_sub(1);
        if self.tries == 0 {
            self.next = None;
            return true;
        }
        self.checks = CHECKS;
        self.next = Some(now + TRY_EVERY);
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Misses every check, each as soon as it is due; the times of the
    /// checks.
    fn miss_all(schedule: &mut Schedule) -> Vec<Instant> {
        let mut checks = Vec::new();
        while let Some(at) = schedule.next() {
            assert!(schedule.due(at));
            checks.push(at);
            if schedule.missed(at) {
                break;
            }
        }
        checks
    }

    #[test]
    fn the_first_check_is_due_at_the_start_and_not_before() {
        let clock = Instant::now();
        let start = clock + Duration::from_secs(30);
        let schedule = Schedule::new(start);
        assert!(!schedule.due(clock));
        assert!(schedule.due(start));
        assert_eq!(schedule.next(), Some(start));
    }

    #[test]
    fn checks_that_find_a_thread_inside_wait_a_second_each_within_a_try() {
        let clock = Instant::now();
        let mut schedule = Schedule::new(clock);
        assert!(!schedule.missed(clock));
        assert!(!schedule.due(clock + Duration::from_millis(999)));
        assert!(schedule.due(clock + CHECK_EVERY));
    }

    #[test]
    fn four_tries_a_minute_apart_then_the_schedule_is_over() {
        let clock = Instant::now();
        let mut schedule = Schedule::new(clock);
        let checks = miss_all(&mut schedule);
        assert_eq!(checks.len(), usize::from(TRIES) * usize::from(CHECKS));
        let starts: Vec<Instant> = checks
            .chunks(usize::from(CHECKS))
            .map(|try_checks| try_checks[0])
            .collect();
        let last_check = CHECK_EVERY * u32::from(CHECKS - 1);
        for pair in starts.windows(2) {
            assert_eq!(pair[1] - pair[0], last_check + TRY_EVERY);
        }
        assert_eq!(schedule.next(), None);
        assert!(!schedule.due(clock + Duration::from_hours(1)));
    }
}
