//! Memory the loader handed the game, freed once the game is done with it.
//!
//! Some game functions the loader answers read the answer only until their
//! caller returns, inside one step of the game's world. Such an answer can
//! go once the world has stepped past it: [`retire`] notes it with the
//! current step, and [`step`] drops what was retired [`GRACE`] steps or more
//! before. A game whose world never steps keeps what it was handed.

use std::any::Any;
use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::locks::Relock;

/// Steps an answer outlives its retirement: the caller that got it returned
/// within the step it asked in, and one more step covers a caller on another
/// thread that asked just before the step ended.
pub(crate) const GRACE: u64 = 2;

/// The world steps seen so far.
static STEP: AtomicU64 = AtomicU64::new(0);

/// What was retired, oldest first, with the step it was retired in.
static RETIRED: Mutex<VecDeque<(u64, Box<dyn Any + Send>)>> = Mutex::new(VecDeque::new());

/// The world steps seen so far, for a cache that renews itself each step.
pub(crate) fn current() -> u64 {
    STEP.load(Ordering::Acquire)
}

/// Keeps `answer` alive for [`GRACE`] more steps, then drops it.
pub(crate) fn retire(answer: impl Any + Send) {
    let step = current();
    RETIRED.relock().push_back((step, Box::new(answer)));
}

/// Notes a step of the game's own world, on its game thread after the step,
/// and drops what is old enough. The answers are dropped outside the lock.
pub(crate) fn step() {
    let now = STEP.fetch_add(1, Ordering::AcqRel) + 1;
    let gone: Vec<_> = {
        let mut retired = RETIRED.relock();
        let mut gone = Vec::new();
        while let Some((at, _)) = retired.front() {
            if *at + GRACE > now {
                break;
            }
            gone.extend(retired.pop_front());
        }
        gone
    };
    drop(gone);
}

/// How many answers wait to be dropped.
#[cfg(test)]
fn waiting() -> usize {
    RETIRED.relock().len()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    #[test]
    fn an_answer_lives_through_its_grace_and_goes_after() {
        let answer = Arc::new([1_u64, 2, 3]);
        let watch = Arc::downgrade(&answer);
        let before = waiting();
        retire(answer);
        assert_eq!(waiting(), before + 1);
        step();
        assert!(watch.upgrade().is_some(), "dropped one step after");
        step();
        assert!(watch.upgrade().is_none(), "kept past its grace");
    }
}
