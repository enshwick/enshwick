//! Waits for transactions' ends: the callbacks mods asked for, each run once
//! after the step that ends its wait.
//!
//! A wait ends when its transaction is no longer pending (the loader turns
//! one applied only once each prop it placed stands in the world), when the
//! loader forgot it, or when its time passed.

use std::time::Instant;

use enshwick_core::abi::TransactionInfo;

use super::Standing;
use crate::engine::mod_index::ModIndex;

/// What a wait runs at its end, with where its transaction stands, `None`
/// once the loader forgot it; the host layer makes it from the mod's
/// callback.
pub(crate) type End = Box<dyn FnOnce(Option<&Standing>) + Send>;

/// A mod's wait for the end of one of its transactions.
pub(crate) struct Wait {
    pub(crate) owner: ModIndex,
    pub(crate) id: u64,
    /// From when a step runs it whatever the transaction's state; `None`
    /// for never.
    until: Option<Instant>,
    end: End,
}

impl Wait {
    pub(crate) fn new(owner: ModIndex, id: u64, until: Option<Instant>, end: End) -> Self {
        Self {
            owner,
            id,
            until,
            end,
        }
    }

    /// Runs its end with where the transaction stands.
    pub(crate) fn run(self, standing: Option<&Standing>) {
        (self.end)(standing);
    }
}

impl core::fmt::Debug for Wait {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Wait")
            .field("owner", &self.owner)
            .field("id", &self.id)
            .field("until", &self.until)
            .finish_non_exhaustive()
    }
}

/// The waits mods asked for, in the order asked.
#[derive(Debug, Default)]
pub(crate) struct Waits(Vec<Wait>);

impl Waits {
    pub(crate) const fn new() -> Self {
        Self(Vec::new())
    }

    pub(crate) fn add(&mut self, wait: Wait) {
        self.0.push(wait);
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Takes out, in the order asked, the waits that end at `now`, each with
    /// where its transaction stands as `standing` gives it, and those of a
    /// mod `off` names, which no step may run. Allocates only when one ends.
    pub(crate) fn due(
        &mut self,
        now: Instant,
        standing: impl Fn(ModIndex, u64) -> Option<Standing>,
        off: impl Fn(ModIndex) -> bool,
    ) -> Vec<(Wait, Option<Standing>)> {
        self.0
            .extract_if(.., |wait| {
                off(wait.owner)
                    || wait.until.is_some_and(|until| now >= until)
                    || standing(wait.owner, wait.id)
                        .is_none_or(|s| s.state != TransactionInfo::PENDING)
            })
            .map(|wait| {
                let standing = standing(wait.owner, wait.id);
                (wait, standing)
            })
            .collect()
    }

    /// Takes out every wait, as when the transactions turn off.
    pub(crate) fn take_all(&mut self) -> Vec<Wait> {
        core::mem::take(&mut self.0)
    }
}

#[cfg(test)]
mod tests {
    use core::time::Duration;

    use super::*;

    const ME: ModIndex = ModIndex::new(0);

    fn wait(id: u64, until: Option<Instant>) -> Wait {
        Wait::new(ME, id, until, Box::new(|_| {}))
    }

    fn standing(state: u32) -> Standing {
        Standing {
            state,
            reason: None,
            ops: 1,
            applied: 0,
            bricks: 0,
            refusal: 0,
            holds: 0,
            created: 0,
        }
    }

    fn ids(due: &[(Wait, Option<Standing>)]) -> Vec<u64> {
        due.iter().map(|(wait, _)| wait.id).collect()
    }

    #[test]
    fn a_wait_ends_once_its_transaction_is_no_longer_pending() {
        let now = Instant::now();
        let mut waits = Waits::new();
        waits.add(wait(1, None));
        waits.add(wait(2, None));
        let state = |_, id| {
            Some(standing(if id == 2 {
                TransactionInfo::APPLIED
            } else {
                TransactionInfo::PENDING
            }))
        };
        assert_eq!(ids(&waits.due(now, state, |_| false)), [2]);
        assert!(!waits.is_empty(), "the pending one waits on");
        let applied = |_, _| Some(standing(TransactionInfo::UNDOING));
        assert_eq!(ids(&waits.due(now, applied, |_| false)), [1]);
        assert!(waits.is_empty());
    }

    #[test]
    fn a_wait_ends_with_no_standing_once_the_loader_forgot_its_transaction() {
        let mut waits = Waits::new();
        waits.add(wait(7, None));
        let due = waits.due(Instant::now(), |_, _| None, |_| false);
        assert_eq!(ids(&due), [7]);
        assert!(due[0].1.is_none());
    }

    #[test]
    fn a_wait_whose_time_passed_ends_with_its_transaction_still_pending() {
        let now = Instant::now();
        let mut waits = Waits::new();
        waits.add(wait(1, Some(now)));
        waits.add(wait(2, now.checked_add(Duration::from_secs(60))));
        let pending = |_, _| Some(standing(TransactionInfo::PENDING));
        let due = waits.due(now, pending, |_| false);
        assert_eq!(ids(&due), [1]);
        assert_eq!(
            due[0].1.as_ref().map(|s| s.state),
            Some(TransactionInfo::PENDING)
        );
    }

    #[test]
    fn due_waits_come_out_in_the_order_asked_and_the_rest_keep_theirs() {
        let mut waits = Waits::new();
        for id in 1..=5 {
            waits.add(wait(id, None));
        }
        let odd = |_, id: u64| {
            Some(standing(if id % 2 == 1 {
                TransactionInfo::FAILED
            } else {
                TransactionInfo::PENDING
            }))
        };
        assert_eq!(ids(&waits.due(Instant::now(), odd, |_| false)), [1, 3, 5]);
        let ids: Vec<u64> = waits.take_all().iter().map(|wait| wait.id).collect();
        assert_eq!(ids, [2, 4]);
    }

    #[test]
    fn the_waits_of_a_mod_that_is_off_leave_at_the_next_step_still_pending() {
        let off = ModIndex::new(1);
        let mut waits = Waits::new();
        waits.add(wait(1, None));
        waits.add(Wait::new(off, 2, None, Box::new(|_| {})));
        let pending = |_, _| Some(standing(TransactionInfo::PENDING));
        let due = waits.due(Instant::now(), pending, |owner| owner == off);
        assert_eq!(ids(&due), [2]);
        assert_eq!(
            ids(&waits.due(Instant::now(), pending, |owner| owner == off)),
            [] as [u64; 0],
            "the other mod's wait stays"
        );
        assert!(!waits.is_empty());
    }
}
