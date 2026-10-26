//! Memory the loader handed the game, freed once the game is done with it.
//!
//! Some game functions the loader answers read the answer only until their
//! caller returns, inside one step of the game's world. Such an answer can
//! go once the world has stepped past it: [`retire`] notes it with the
//! current step and time, and [`step`] drops what was retired at least
//! [`GRACE`] steps and [`MIN_AGE`] before. A game whose world never steps
//! keeps what it was handed.

use std::any::Any;
use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use crate::locks::Relock;

/// Steps an answer outlives its retirement: the caller that got it returned
/// within the step it asked in, and one more step covers a caller on another
/// thread that asked just before the step ended.
pub(crate) const GRACE: u64 = 2;

/// The least time an answer outlives its retirement, whatever the steps: a
/// caller on another thread may be held off its core for a scheduler slice
/// or more while a fast client steps several times.
pub(crate) const MIN_AGE: Duration = Duration::from_secs(1);

/// Answers waiting to be dropped, and the steps seen so far.
#[derive(Default)]
struct Retired {
    /// The world steps seen so far.
    step: u64,
    /// What was retired, oldest first, with the step and time it was
    /// retired at.
    queue: VecDeque<(u64, Instant, Box<dyn Any + Send>)>,
}

impl Retired {
    const fn new() -> Self {
        Self {
            step: 0,
            queue: VecDeque::new(),
        }
    }

    fn retire(&mut self, now: Instant, answer: Box<dyn Any + Send>) {
        self.queue.push_back((self.step, now, answer));
    }

    /// Counts a step and takes out what is old enough in steps and time.
    fn step(&mut self, now: Instant) -> Vec<Box<dyn Any + Send>> {
        self.step += 1;
        let mut gone = Vec::new();
        while let Some(&(at, when, _)) = self.queue.front() {
            if at + GRACE > self.step || now.saturating_duration_since(when) < MIN_AGE {
                break;
            }
            gone.extend(self.queue.pop_front().map(|(_, _, answer)| answer));
        }
        gone
    }
}

/// The steps seen so far, mirrored out of [`RETIRED`] for a cache that
/// renews itself each step without taking the lock.
static STEP: AtomicU64 = AtomicU64::new(0);

static RETIRED: Mutex<Retired> = Mutex::new(Retired::new());

/// The world steps seen so far, for a cache that renews itself each step.
pub(crate) fn current() -> u64 {
    STEP.load(Ordering::Acquire)
}

/// Keeps `answer` alive for at least [`GRACE`] more steps and [`MIN_AGE`],
/// then drops it.
pub(crate) fn retire(answer: impl Any + Send) {
    let answer: Box<dyn Any + Send> = Box::new(answer);
    RETIRED.relock().retire(Instant::now(), answer);
}

/// Notes a step of the game's own world, on its game thread after the step,
/// and drops what is old enough, outside the lock.
pub(crate) fn step() {
    let gone = {
        let mut retired = RETIRED.relock();
        let gone = retired.step(Instant::now());
        STEP.store(retired.step, Ordering::Release);
        gone
    };
    drop(gone);
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    #[test]
    fn an_answer_goes_only_after_its_steps_and_its_time() {
        let mut retired = Retired::new();
        let start = Instant::now();
        let answer = Arc::new([1_u64, 2, 3]);
        let watch = Arc::downgrade(&answer);
        retired.retire(start, Box::new(answer));
        assert!(retired.step(start).is_empty());
        assert!(retired.step(start).is_empty(), "two steps but no time");
        assert_eq!(retired.step(start + MIN_AGE).len(), 1);
        assert!(watch.upgrade().is_none());
    }

    #[test]
    fn an_old_answer_still_waits_for_its_steps() {
        let mut retired = Retired::new();
        let start = Instant::now();
        retired.retire(start, Box::new(7_u32));
        assert!(retired.step(start + MIN_AGE * 10).is_empty());
        assert_eq!(retired.step(start + MIN_AGE * 10).len(), 1);
        assert!(retired.queue.is_empty());
    }
}
