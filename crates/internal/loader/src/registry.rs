//! What mods register during init, open until the loader seals it and read
//! without a lock after.

use std::sync::{Mutex, OnceLock};

use crate::locks::Relock;

/// A registry is sealed: what mods ask for after init is refused.
///
/// # Examples
///
/// ```
/// use enshwick_loader::registry::{Registry, Sealed};
///
/// let registry: Registry<u32> = Registry::new();
/// registry.seal();
/// assert_eq!(registry.add(1), Err(Sealed));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the registry is sealed")]
pub struct Sealed;

impl From<Sealed> for enshwick::abi::Status {
    fn from(_: Sealed) -> Self {
        Self::TOO_LATE
    }
}

/// Items mods register while the loader starts: open for additions until
/// [`Registry::seal`], then a fixed slice that hot paths read without a lock.
///
/// # Examples
///
/// ```
/// use enshwick_loader::registry::Registry;
///
/// static FILTERS: Registry<u32> = Registry::new();
/// FILTERS.add(7)?;
/// assert_eq!(FILTERS.seal(), [7]);
/// assert_eq!(FILTERS.sealed(), Some(&[7][..]));
/// # Ok::<(), enshwick_loader::registry::Sealed>(())
/// ```
#[derive(Debug)]
pub struct Registry<T> {
    open: Mutex<Vec<T>>,
    sealed: OnceLock<Box<[T]>>,
}

impl<T> Registry<T> {
    /// An open, empty registry, for a `static`.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            open: Mutex::new(Vec::new()),
            sealed: OnceLock::new(),
        }
    }

    /// Adds `item`.
    ///
    /// # Errors
    ///
    /// [`Sealed`] once the registry is sealed.
    pub fn add(&self, item: T) -> Result<(), Sealed> {
        self.edit(|items| items.push(item))
    }

    /// Runs `edit` on the items registered so far, for checks across them.
    ///
    /// # Errors
    ///
    /// [`Sealed`] once the registry is sealed.
    pub fn edit<R>(&self, edit: impl FnOnce(&mut Vec<T>) -> R) -> Result<R, Sealed> {
        let mut open = self.open.relock();
        if self.sealed.get().is_some() {
            return Err(Sealed);
        }
        Ok(edit(&mut open))
    }

    /// Whether the registry is sealed.
    #[must_use]
    pub fn is_sealed(&self) -> bool {
        self.sealed.get().is_some()
    }

    /// Seals the registry and returns what was registered; later calls
    /// return the same slice.
    pub fn seal(&self) -> &[T] {
        let mut open = self.open.relock();
        self.sealed
            .get_or_init(|| std::mem::take(&mut *open).into_boxed_slice())
    }

    /// The sealed items; `None` while the registry is open.
    #[inline]
    #[must_use]
    pub fn sealed(&self) -> Option<&[T]> {
        self.sealed.get().map(|items| &**items)
    }
}

impl<T> Default for Registry<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sealed_registry_refuses_additions_and_keeps_its_items() {
        let registry = Registry::new();
        assert_eq!(registry.sealed(), None);
        assert_eq!(registry.add(1), Ok(()));
        assert_eq!(registry.edit(|items| items.len()), Ok(1));
        assert_eq!(registry.seal(), [1]);
        assert!(registry.is_sealed());
        assert_eq!(registry.add(2), Err(Sealed));
        assert_eq!(registry.seal(), [1]);
        assert_eq!(registry.sealed(), Some(&[1][..]));
    }
}
