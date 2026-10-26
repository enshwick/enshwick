//! The loader's one lock poison policy: a lock a panicking thread held is
//! taken again with its state kept, its poison cleared and the place logged.

use std::panic::Location;
use std::sync::{Mutex, MutexGuard};

/// Takes a lock under the poison policy.
pub(crate) trait Relock<T: ?Sized> {
    /// The guard, also after a thread panicked holding the lock.
    fn relock(&self) -> MutexGuard<'_, T>;
}

impl<T: ?Sized> Relock<T> for Mutex<T> {
    #[track_caller]
    #[inline]
    fn relock(&self) -> MutexGuard<'_, T> {
        self.lock().unwrap_or_else(|poisoned| {
            poisoned_at(Location::caller());
            self.clear_poison();
            poisoned.into_inner()
        })
    }
}

/// Logs that the lock taken at `at` was poisoned.
#[cold]
fn poisoned_at(at: &Location<'_>) {
    crate::log!(
        WARN,
        "{}:{}: a thread panicked holding this lock; its state is kept",
        at.file(),
        at.line()
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_poisoned_lock_keeps_its_state_and_heals() {
        let lock = Mutex::new(vec![1]);
        let _panicked = std::panic::catch_unwind(|| {
            let mut held = lock.lock().unwrap();
            held.push(2);
            panic!("while holding");
        });
        assert!(lock.is_poisoned());
        assert_eq!(*lock.relock(), [1, 2]);
        assert!(!lock.is_poisoned());
    }
}
