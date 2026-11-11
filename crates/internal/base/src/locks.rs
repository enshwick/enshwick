//! The API's one lock poison policy: a lock a panicking thread held is
//! taken again with its state kept, its poison cleared and the place logged.

use std::panic::Location;
use std::sync::{
    Condvar, Mutex, MutexGuard, OnceLock, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard,
    TryLockError, WaitTimeoutResult,
};
use std::time::Duration;

mod sealed {
    pub trait Sealed {}
    impl<T: ?Sized> Sealed for std::sync::Mutex<T> {}
    impl<T: ?Sized> Sealed for std::sync::RwLock<T> {}
    impl Sealed for std::sync::Condvar {}
}

/// Takes a [`Mutex`] after a panic too, so one panicking thread does not
/// turn the lock off for every other; use it instead of `lock()`.
///
/// A poisoned lock keeps the state the panicking thread left: its poison is
/// cleared and the place logged as a warning.
///
/// # Examples
///
/// ```
/// # extern crate enshwick_base as enshwick;
/// use std::sync::Mutex;
///
/// use enshwick::Relock as _;
///
/// let seen = Mutex::new(Vec::new());
/// seen.relock().push(7);
/// assert_eq!(*seen.relock(), [7]);
/// ```
pub trait Relock<T: ?Sized>: sealed::Sealed {
    /// The guard, also after a thread panicked holding the lock.
    fn relock(&self) -> MutexGuard<'_, T>;
    /// The guard if no one holds the lock, also after a thread panicked
    /// holding it; `None` while it is held, by this thread too.
    fn try_relock(&self) -> Option<MutexGuard<'_, T>>;
}

/// Takes an [`RwLock`] after a panic too, as [`Relock`] takes a
/// [`Mutex`]; use it instead of `read()` and `write()`.
///
/// # Examples
///
/// ```
/// # extern crate enshwick_base as enshwick;
/// use std::sync::RwLock;
///
/// use enshwick::RelockRw as _;
///
/// let limit = RwLock::new(4);
/// *limit.relock_write() += 1;
/// assert_eq!(*limit.relock_read(), 5);
/// ```
pub trait RelockRw<T: ?Sized>: sealed::Sealed {
    /// The read guard, also after a thread panicked holding the lock.
    fn relock_read(&self) -> RwLockReadGuard<'_, T>;
    /// The write guard, also after a thread panicked holding the lock.
    fn relock_write(&self) -> RwLockWriteGuard<'_, T>;
}

/// Waits on a [`Condvar`] after a panic too, as [`Relock`] takes its
/// [`Mutex`]; use it instead of `wait()` and its kin.
///
/// A wait that wakes on a poisoned lock hands back the guard with the state
/// kept and logs the place; the lock's poison is cleared by its next
/// [`Relock::relock`].
///
/// # Panics
///
/// Each wait panics as [`Condvar::wait`] does: when the condition variable
/// is used with more than one mutex.
///
/// # Examples
///
/// ```
/// # extern crate enshwick_base as enshwick;
/// use std::sync::{Condvar, Mutex};
/// use std::time::Duration;
///
/// use enshwick::{Relock as _, RelockWait as _};
///
/// let stop = Mutex::new(true);
/// let wake = Condvar::new();
/// let (stopped, _timeout) =
///     wake.relock_wait_timeout_while(stop.relock(), Duration::from_secs(1), |stop| !*stop);
/// assert!(*stopped);
/// ```
pub trait RelockWait: sealed::Sealed {
    /// [`Condvar::wait`], with the guard also on a poisoned lock.
    fn relock_wait<'a, T>(&self, guard: MutexGuard<'a, T>) -> MutexGuard<'a, T>;
    /// [`Condvar::wait_while`], with the guard also on a poisoned lock.
    fn relock_wait_while<'a, T>(
        &self,
        guard: MutexGuard<'a, T>,
        condition: impl FnMut(&mut T) -> bool,
    ) -> MutexGuard<'a, T>;
    /// [`Condvar::wait_timeout`], with the guard also on a poisoned lock.
    fn relock_wait_timeout<'a, T>(
        &self,
        guard: MutexGuard<'a, T>,
        timeout: Duration,
    ) -> (MutexGuard<'a, T>, WaitTimeoutResult);
    /// [`Condvar::wait_timeout_while`], with the guard also on a poisoned
    /// lock.
    fn relock_wait_timeout_while<'a, T>(
        &self,
        guard: MutexGuard<'a, T>,
        timeout: Duration,
        condition: impl FnMut(&mut T) -> bool,
    ) -> (MutexGuard<'a, T>, WaitTimeoutResult);
}

impl<T: ?Sized> Relock<T> for Mutex<T> {
    #[track_caller]
    #[inline]
    fn relock(&self) -> MutexGuard<'_, T> {
        let at = Location::caller();
        self.lock().unwrap_or_else(|poisoned| {
            let_go(poisoned, || self.clear_poison(), at);
            self.lock().unwrap_or_else(PoisonError::into_inner)
        })
    }

    #[track_caller]
    #[inline]
    fn try_relock(&self) -> Option<MutexGuard<'_, T>> {
        match self.try_lock() {
            Ok(guard) => Some(guard),
            Err(TryLockError::WouldBlock) => None,
            Err(TryLockError::Poisoned(poisoned)) => {
                let_go(poisoned, || self.clear_poison(), Location::caller());
                match self.try_lock() {
                    Ok(guard) => Some(guard),
                    Err(TryLockError::WouldBlock) => None,
                    Err(TryLockError::Poisoned(again)) => Some(again.into_inner()),
                }
            }
        }
    }
}

impl<T: ?Sized> RelockRw<T> for RwLock<T> {
    #[track_caller]
    #[inline]
    fn relock_read(&self) -> RwLockReadGuard<'_, T> {
        let at = Location::caller();
        self.read().unwrap_or_else(|poisoned| {
            let_go(poisoned, || self.clear_poison(), at);
            self.read().unwrap_or_else(PoisonError::into_inner)
        })
    }

    #[track_caller]
    #[inline]
    fn relock_write(&self) -> RwLockWriteGuard<'_, T> {
        let at = Location::caller();
        self.write().unwrap_or_else(|poisoned| {
            let_go(poisoned, || self.clear_poison(), at);
            self.write().unwrap_or_else(PoisonError::into_inner)
        })
    }
}

/// Clears the poison of the lock `poisoned` holds, lets the lock go and
/// reports `at`, so the report may take that lock too, as the loader's log
/// takes its own; the caller takes the lock again.
#[cold]
fn let_go<G>(poisoned: PoisonError<G>, clear: impl FnOnce(), at: &Location<'_>) {
    clear();
    drop(poisoned);
    poisoned_at(at);
}

impl RelockWait for Condvar {
    #[track_caller]
    #[inline]
    fn relock_wait<'a, T>(&self, guard: MutexGuard<'a, T>) -> MutexGuard<'a, T> {
        let at = Location::caller();
        self.wait(guard)
            .unwrap_or_else(|poisoned| woke_poisoned(poisoned, at))
    }

    #[track_caller]
    #[inline]
    fn relock_wait_while<'a, T>(
        &self,
        guard: MutexGuard<'a, T>,
        condition: impl FnMut(&mut T) -> bool,
    ) -> MutexGuard<'a, T> {
        let at = Location::caller();
        self.wait_while(guard, condition)
            .unwrap_or_else(|poisoned| woke_poisoned(poisoned, at))
    }

    #[track_caller]
    #[inline]
    fn relock_wait_timeout<'a, T>(
        &self,
        guard: MutexGuard<'a, T>,
        timeout: Duration,
    ) -> (MutexGuard<'a, T>, WaitTimeoutResult) {
        let at = Location::caller();
        self.wait_timeout(guard, timeout)
            .unwrap_or_else(|poisoned| woke_poisoned(poisoned, at))
    }

    #[track_caller]
    #[inline]
    fn relock_wait_timeout_while<'a, T>(
        &self,
        guard: MutexGuard<'a, T>,
        timeout: Duration,
        condition: impl FnMut(&mut T) -> bool,
    ) -> (MutexGuard<'a, T>, WaitTimeoutResult) {
        let at = Location::caller();
        self.wait_timeout_while(guard, timeout, condition)
            .unwrap_or_else(|poisoned| woke_poisoned(poisoned, at))
    }
}

/// What a wait at `at` that woke on a poisoned lock hands back, logged.
#[cold]
fn woke_poisoned<G>(poisoned: PoisonError<G>, at: &Location<'_>) -> G {
    poisoned_at(at);
    poisoned.into_inner()
}

/// Where a poisoned lock's place goes instead of a mod's log, named once by
/// a crate with a log of its own, the loader.
static SINK: OnceLock<fn(&Location<'_>)> = OnceLock::new();

/// Sends the place of each poisoned lock to `sink` instead of a mod's log;
/// the first call wins.
pub(crate) fn report_to(sink: fn(&Location<'_>)) {
    let _first = SINK.set(sink);
}

/// Reports that the lock taken at `at` was poisoned: in a data pack's code
/// to the pack's log, as its other lines, else to the sink if one is named.
#[cold]
fn poisoned_at(at: &Location<'_>) {
    if crate::host::pack().is_none()
        && let Some(sink) = SINK.get()
    {
        sink(at);
        return;
    }
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

    /// A lock a thread panicked holding, after it set the state to `to`.
    fn poisoned<T>(lock: &Mutex<T>, to: T) {
        let _panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut held = lock.lock().unwrap();
            *held = to;
            panic!("while holding");
        }));
        assert!(lock.is_poisoned());
    }

    #[test]
    fn a_poisoned_lock_keeps_its_state_and_heals() {
        let lock = Mutex::new(vec![1]);
        poisoned(&lock, vec![1, 2]);
        assert_eq!(*lock.relock(), [1, 2]);
        assert!(!lock.is_poisoned());
    }

    #[test]
    fn a_named_sink_hears_where_a_poisoned_lock_was_taken_once_it_is_let_go() {
        static LOCK: Mutex<u32> = Mutex::new(0);
        thread_local! {
            // Per thread: the sink is the process's, and other tests poison locks too.
            static HEARD: std::cell::Cell<Option<(u32, bool)>> = const { std::cell::Cell::new(None) };
        }
        crate::__internal::report_poison_to(|at| {
            let free = !matches!(LOCK.try_lock(), Err(TryLockError::WouldBlock));
            HEARD.set(Some((at.line(), free)));
        });
        poisoned(&LOCK, 1);
        let line = line!() + 1;
        assert_eq!(*LOCK.relock(), 1);
        assert_eq!(HEARD.get(), Some((line, true)));
        poisoned(&LOCK, 2);
        let line = line!() + 1;
        assert_eq!(LOCK.try_relock().as_deref(), Some(&2));
        assert_eq!(HEARD.get(), Some((line, true)));
    }

    #[test]
    fn a_poisoned_lock_is_tried_with_its_state_kept_and_healed() {
        let lock = Mutex::new(vec![1]);
        poisoned(&lock, vec![1, 2]);
        assert_eq!(lock.try_relock().as_deref(), Some(&vec![1, 2]));
        assert!(!lock.is_poisoned());
        let held = lock.relock();
        assert!(lock.try_relock().is_none(), "held");
        drop(held);
        assert!(lock.try_relock().is_some());
    }

    #[test]
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

    #[test]
    fn a_wait_on_a_poisoned_lock_keeps_its_state_until_the_next_relock_heals_it() {
        let lock = Mutex::new(false);
        let wake = Condvar::new();
        poisoned(&lock, true);
        let guard = lock.lock().unwrap_or_else(PoisonError::into_inner);
        let stop = wake.relock_wait_while(guard, |stop| !*stop);
        assert!(*stop);
        let (stop, waited) = wake.relock_wait_timeout_while(stop, Duration::ZERO, |stop| !*stop);
        assert!(*stop && !waited.timed_out());
        let (stop, waited) = wake.relock_wait_timeout(stop, Duration::from_millis(1));
        assert!(*stop && waited.timed_out());
        drop(stop);
        assert!(lock.is_poisoned());
        assert!(*lock.relock());
        assert!(!lock.is_poisoned());
    }

    #[test]
    fn a_wait_wakes_when_another_thread_notifies() {
        let lock = Mutex::new(false);
        let wake = Condvar::new();
        std::thread::scope(|scope| {
            scope.spawn(|| {
                *lock.relock() = true;
                wake.notify_all();
            });
            let mut ready = lock.relock();
            while !*ready {
                ready = wake.relock_wait(ready);
            }
            assert!(*ready);
        });
    }
}
