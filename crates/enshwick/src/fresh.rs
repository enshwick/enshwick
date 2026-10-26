//! Process state the fake host starts afresh on every install.
//!
//! In the game a mod's state lives once per load of its DLL. Under the fake
//! host each install is a new mod life, so state kept in a [`Fresh`] cell
//! from an earlier install reads as empty. Values are never dropped: a
//! reference handed out stays valid for the process, and an earlier life's
//! value is left in place, unreachable.

#[cfg(feature = "test")]
use std::sync::atomic::{AtomicPtr, AtomicU64, Ordering};

/// The mod life the process is in; every install of the fake starts the
/// next.
#[cfg(feature = "test")]
static LIFE: AtomicU64 = AtomicU64::new(1);

/// The mod life the process is in.
#[cfg(feature = "test")]
pub(crate) fn life() -> u64 {
    LIFE.load(Ordering::Acquire)
}

/// The mod life the process is in: the only one in the game.
#[cfg(not(feature = "test"))]
#[inline]
pub(crate) const fn life() -> u64 {
    1
}

/// Starts the next mod life: every [`Fresh`] cell reads empty again.
#[cfg(feature = "test")]
pub(crate) fn renew() {
    LIFE.fetch_add(1, Ordering::AcqRel);
}

/// A cell set once per mod life, like a `OnceLock`.
#[derive(Debug)]
pub(crate) struct Fresh<T> {
    #[cfg(not(feature = "test"))]
    cell: std::sync::OnceLock<T>,
    /// The value with the life that set it, leaked.
    #[cfg(feature = "test")]
    cell: AtomicPtr<(u64, T)>,
}

#[cfg(not(feature = "test"))]
impl<T> Fresh<T> {
    pub(crate) const fn new() -> Self {
        Self {
            cell: std::sync::OnceLock::new(),
        }
    }

    pub(crate) fn get(&self) -> Option<&T> {
        self.cell.get()
    }

    /// Keeps `value`; hands it back when the cell holds one already.
    pub(crate) fn set(&self, value: T) -> Result<(), T> {
        self.cell.set(value)
    }

    pub(crate) fn get_or_init(&self, init: impl FnOnce() -> T) -> &T {
        self.cell.get_or_init(init)
    }
}

#[cfg(feature = "test")]
impl<T> Fresh<T> {
    pub(crate) const fn new() -> Self {
        Self {
            cell: AtomicPtr::new(core::ptr::null_mut()),
        }
    }

    pub(crate) fn get(&self) -> Option<&T> {
        let kept = self.cell.load(Ordering::Acquire);
        // SAFETY: a non-null pointer comes from `Box::into_raw` in `set`, and
        // what it points to is never freed or written again.
        let (set, value) = unsafe { kept.as_ref() }?;
        (*set == life()).then_some(value)
    }

    /// Keeps `value`; hands it back when the cell holds one for this life
    /// already.
    pub(crate) fn set(&self, value: T) -> Result<(), T> {
        self.keep(value).map(|_| ())
    }

    pub(crate) fn get_or_init(&self, init: impl FnOnce() -> T) -> &T {
        if let Some(value) = self.get() {
            return value;
        }
        let mut value = init();
        loop {
            match self.keep(value) {
                Ok(kept) => return kept,
                Err(lost) => match self.get() {
                    Some(kept) => return kept,
                    None => value = lost,
                },
            }
        }
    }

    /// Keeps `value` for this life and lends it back; hands it back when the
    /// cell holds one for this life already or another thread set it first.
    fn keep(&self, value: T) -> Result<&T, T> {
        let kept = self.cell.load(Ordering::Acquire);
        // SAFETY: a non-null pointer comes from `Box::into_raw` below, and
        // what it points to is never freed or written again.
        if unsafe { kept.as_ref() }.is_some_and(|(set, _)| *set == life()) {
            return Err(value);
        }
        let fresh = Box::into_raw(Box::new((life(), value)));
        if self
            .cell
            .compare_exchange(kept, fresh, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            // SAFETY: `fresh` came from `Box::into_raw` above and no other
            // thread saw it.
            let (_, value) = *unsafe { Box::from_raw(fresh) };
            return Err(value);
        }
        // SAFETY: `fresh` is now kept in the cell, never freed or written
        // again.
        Ok(unsafe { &(*fresh).1 })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "test")]
    #[test]
    fn a_cell_reads_empty_in_the_next_life_and_keeps_old_references() {
        static CELL: Fresh<u32> = Fresh::new();
        let first = CELL.get_or_init(|| 1);
        assert_eq!(CELL.set(2), Err(2), "one value per life");
        renew();
        assert_eq!(CELL.get(), None);
        assert_eq!(CELL.get_or_init(|| 3), &3);
        assert_eq!(*first, 1, "an earlier life's value stays readable");
    }
}
