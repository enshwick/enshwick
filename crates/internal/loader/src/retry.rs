//! Code patches a thread kept out at arming, tried again from the game step:
//! at the next world load, then up to three more times a minute apart. Their
//! owners wait meanwhile, with their hooks bypassed; after the last try they
//! are turned off.

use std::time::{Duration, Instant};

#[cfg(windows)]
mod run;

#[cfg(windows)]
pub(crate) use run::{Retry, after_step, hold, world_loaded};

/// Tries: the one at the next world load and three more.
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
    /// The next check; `None` until a world loads.
    next: Option<Instant>,
}

impl Schedule {
    /// A schedule waiting for a world to load.
    pub(crate) const fn new() -> Self {
        Self {
            tries: TRIES,
            checks: CHECKS,
            next: None,
        }
    }

    /// Starts the first try at `now`, when a world loads; a later load
    /// changes nothing.
    pub(crate) fn world_loaded(&mut self, now: Instant) {
        if self.next.is_none() && self.tries == TRIES && self.checks == CHECKS {
            self.next = Some(now);
        }
    }

    /// The next check; `None` while no world has loaded or after the last.
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
    fn nothing_is_checked_before_a_world_loads() {
        let clock = Instant::now();
        let schedule = Schedule::new();
        assert_eq!(schedule.next(), None);
        assert!(!schedule.due(clock + Duration::from_hours(1)));
    }

    #[test]
    fn the_first_check_comes_at_the_world_load_and_not_before() {
        let clock = Instant::now();
        let loaded = clock + Duration::from_secs(30);
        let mut schedule = Schedule::new();
        schedule.world_loaded(loaded);
        assert!(!schedule.due(clock));
        assert!(schedule.due(loaded));
        schedule.world_loaded(loaded + Duration::from_secs(5));
        assert_eq!(schedule.next(), Some(loaded));
    }

    #[test]
    fn checks_that_find_a_thread_inside_wait_a_second_each_within_a_try() {
        let clock = Instant::now();
        let mut schedule = Schedule::new();
        schedule.world_loaded(clock);
        assert!(!schedule.missed(clock));
        assert!(!schedule.due(clock + Duration::from_millis(999)));
        assert!(schedule.due(clock + CHECK_EVERY));
    }

    #[test]
    fn four_tries_a_minute_apart_then_the_schedule_is_over() {
        let clock = Instant::now();
        let mut schedule = Schedule::new();
        schedule.world_loaded(clock);
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

    #[test]
    fn a_world_load_after_the_tries_began_does_not_start_them_again() {
        let clock = Instant::now();
        let mut schedule = Schedule::new();
        schedule.world_loaded(clock);
        miss_all(&mut schedule);
        schedule.world_loaded(clock + Duration::from_hours(1));
        assert_eq!(schedule.next(), None);
    }
}
