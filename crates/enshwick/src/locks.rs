//! The API's one lock poison policy: a lock a panicking thread held is
//! taken again with its state kept, its poison cleared and the place logged.

use std::panic::Location;
use std::sync::{Mutex, MutexGuard};
#[cfg(feature = "unstable")]
use std::sync::{PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard};

/// Takes a lock under the poison policy.
pub trait Relock<T: ?Sized> {
    /// The guard, also after a thread panicked holding the lock.
    fn relock(&self) -> MutexGuard<'_, T>;
}

/// Takes a read-write lock under the poison policy.
#[cfg(feature = "unstable")]
pub(crate) trait RelockRw<T: ?Sized> {
    /// The read guard, also after a thread panicked holding the lock.
    fn relock_read(&self) -> RwLockReadGuard<'_, T>;
    /// The write guard, also after a thread panicked holding the lock.
    fn relock_write(&self) -> RwLockWriteGuard<'_, T>;
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

#[cfg(feature = "unstable")]
impl<T: ?Sized> RelockRw<T> for RwLock<T> {
    #[track_caller]
    #[inline]
    fn relock_read(&self) -> RwLockReadGuard<'_, T> {
        self.read().unwrap_or_else(|poisoned| {
            poisoned_at(Location::caller());
            self.clear_poison();
            PoisonError::into_inner(poisoned)
        })
    }

    #[track_caller]
    #[inline]
    fn relock_write(&self) -> RwLockWriteGuard<'_, T> {
        self.write().unwrap_or_else(|poisoned| {
            poisoned_at(Location::caller());
            self.clear_poison();
            PoisonError::into_inner(poisoned)
        })
    }
}

/// Logs that the lock taken at `at` was poisoned.
#[cold]
fn poisoned_at(at: &Location<'_>) {
    crate::__private::log(
        crate::abi::Level::WARN,
        format_args!(
            "{}:{}: a thread panicked holding this lock; its state is kept",
            at.file(),
            at.line()
        ),
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
    #[cfg(feature = "unstable")]
    fn a_poisoned_read_write_lock_keeps_its_state_and_heals() {
        let lock = RwLock::new(1);
        let _panicked = std::panic::catch_unwind(|| {
            let mut held = lock.write().unwrap();
            *held = 2;
            panic!("while holding");
        });
        assert_eq!(*lock.relock_read(), 2);
        *lock.relock_write() = 3;
        assert!(!lock.is_poisoned());
        assert_eq!(*lock.relock_read(), 3);
    }
}
