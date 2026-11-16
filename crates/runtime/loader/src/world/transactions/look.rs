//! When a step looks at the waits: only once the ledger changed, a mod
//! turned off or a wait's time came since the last look, so a step with
//! waits pending and nothing new takes no lock.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{MutexGuard, OnceLock};
use std::time::Instant;

use super::Ledger;

/// What the last look at the waits saw.
#[derive(Debug)]
pub(super) struct Look {
    /// The ledger's mutable uses so far, counted as each lock drops.
    changes: AtomicU64,
    /// `changes` at the last look; [`ASK`] makes the next step look.
    seen: AtomicU64,
    /// The mods turned off at the last look.
    offs: AtomicU64,
    /// When the earliest wait's time comes, in nanoseconds from `start`;
    /// `u64::MAX` for none.
    bound: AtomicU64,
    start: OnceLock<Instant>,
}

/// A `seen` no count of changes reaches, so the next step looks.
const ASK: u64 = u64::MAX;

impl Look {
    pub(super) const fn new() -> Self {
        Self {
            changes: AtomicU64::new(0),
            seen: AtomicU64::new(ASK),
            offs: AtomicU64::new(0),
            bound: AtomicU64::new(u64::MAX),
            start: OnceLock::new(),
        }
    }

    /// `instant` in nanoseconds from the look's start, 0 before it.
    fn at(&self, instant: Instant) -> u64 {
        let start = *self.start.get_or_init(Instant::now);
        u64::try_from(instant.saturating_duration_since(start).as_nanos()).unwrap_or(u64::MAX)
    }

    /// The ledger's changes so far when a step at `now`, with `offs` mods
    /// turned off, must look; `None` when nothing is new since the last
    /// look.
    pub(super) fn due(&self, now: Instant, offs: u64) -> Option<u64> {
        let changes = self.changes.load(Ordering::Acquire);
        let quiet = changes == self.seen.load(Ordering::Acquire)
            && offs == self.offs.load(Ordering::Acquire)
            && self.at(now) < self.bound.load(Ordering::Acquire);
        (!quiet).then_some(changes)
    }

    /// Notes a look that read `changes` and `offs`, the waits it left
    /// ending by time first at `bound`.
    pub(super) fn looked(&self, changes: u64, offs: u64, bound: Option<Instant>) {
        let bound = bound.map_or(u64::MAX, |bound| self.at(bound));
        self.bound.store(bound, Ordering::Release);
        self.offs.store(offs, Ordering::Release);
        self.seen.store(changes, Ordering::Release);
    }

    /// Makes the next step look, as a new wait does.
    pub(super) fn ask(&self) {
        self.seen.store(ASK, Ordering::Release);
    }
}

/// The ledger's lock, counting a mutable use in its [`Look`] as it drops:
/// every change of a record's state goes through one.
pub(super) struct LedgerGuard<'a> {
    pub(super) guard: MutexGuard<'a, Ledger>,
    pub(super) look: &'a Look,
    pub(super) changed: bool,
}

impl core::ops::Deref for LedgerGuard<'_> {
    type Target = Ledger;

    fn deref(&self) -> &Ledger {
        &self.guard
    }
}

impl core::ops::DerefMut for LedgerGuard<'_> {
    fn deref_mut(&mut self) -> &mut Ledger {
        self.changed = true;
        &mut self.guard
    }
}

impl Drop for LedgerGuard<'_> {
    fn drop(&mut self) {
        if self.changed {
            let _count = self.look.changes.fetch_add(1, Ordering::AcqRel);
        }
    }
}

#[cfg(test)]
mod tests {
    use core::time::Duration;

    use super::*;

    #[test]
    fn a_step_looks_again_only_once_something_is_new() {
        let look = Look::new();
        let now = Instant::now();
        assert_eq!(look.due(now, 0), Some(0), "a first look");
        look.looked(0, 0, None);
        assert_eq!(look.due(now, 0), None, "nothing new");
        look.changes.fetch_add(1, Ordering::AcqRel);
        assert_eq!(look.due(now, 0), Some(1), "the ledger changed");
        look.looked(1, 0, None);
        assert_eq!(look.due(now, 1), Some(1), "a mod turned off");
        look.looked(1, 1, now.checked_add(Duration::from_secs(1)));
        assert_eq!(look.due(now, 1), None);
        let later = now.checked_add(Duration::from_secs(2)).unwrap_or(now);
        assert_eq!(look.due(later, 1), Some(1), "a wait's time came");
        look.ask();
        assert_eq!(look.due(now, 1), Some(1), "a new wait asks");
    }
}
