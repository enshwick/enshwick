//! The loader's one lock poison policy: a lock a panicking thread held is
//! taken again with its state kept, its poison cleared and the place logged.

use std::panic::Location;
use std::sync::{Mutex, MutexGuard, TryLockError};

/// Takes a lock under the poison policy.
pub(crate) trait Relock<T: ?Sized> {
    /// The guard, also after a thread panicked holding the lock.
    fn relock(&self) -> MutexGuard<'_, T>;
    /// The guard without waiting, also after a thread panicked holding the
    /// lock; `None` while another thread holds it.
    #[cfg_attr(
        not(any(windows, test)),
        expect(
            dead_code,
            reason = "only the Windows build's per-step paths try a lock"
        )
    )]
    fn try_relock(&self) -> Option<MutexGuard<'_, T>>;
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

    #[track_caller]
    #[inline]
    fn try_relock(&self) -> Option<MutexGuard<'_, T>> {
        match self.try_lock() {
            Ok(guard) => Some(guard),
            Err(TryLockError::WouldBlock) => None,
            Err(TryLockError::Poisoned(poisoned)) => {
                poisoned_at(Location::caller());
                self.clear_poison();
                Some(poisoned.into_inner())
            }
        }
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

    #[test]
    fn a_poisoned_lock_is_tried_with_its_state_kept_and_healed() {
        let lock = Mutex::new(vec![1]);
        let _panicked = std::panic::catch_unwind(|| {
            let mut held = lock.lock().unwrap();
            held.push(2);
            panic!("while holding");
        });
        assert_eq!(lock.try_relock().as_deref(), Some(&vec![1, 2]));
        assert!(!lock.is_poisoned());
        let held = lock.relock();
        assert!(lock.try_relock().is_none(), "a held lock is not waited for");
        drop(held);
    }
}
