//! What mods register during init, open until the loader seals it and read
//! without a lock after.

use std::sync::{Mutex, OnceLock};

use enshwick_base::Relock;

/// A registry is sealed: what mods ask for after init is refused.
///
/// # Examples
///
/// ```
/// use enshwick_loader::engine::registry::{Registry, Sealed};
///
/// let registry: Registry<u32> = Registry::new();
/// registry.seal();
/// assert_eq!(registry.add(1), Err(Sealed));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the registry is sealed")]
pub struct Sealed;

impl From<Sealed> for enshwick_core::abi::Status {
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
/// use enshwick_loader::engine::registry::Registry;
///
/// static FILTERS: Registry<u32> = Registry::new();
/// FILTERS.add(7)?;
/// assert_eq!(FILTERS.seal(), [7]);
/// assert_eq!(FILTERS.sealed(), Some(&[7][..]));
/// # Ok::<(), enshwick_loader::engine::registry::Sealed>(())
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
        self.seal_with(|items| items)
    }

    /// Seals the registry with what `finish` makes of the items registered,
    /// such as the loader's own entries added: additions from other threads
    /// wait while it runs and are refused after, so none falls between the
    /// items taken and the seal. `finish` must not use this registry, whose
    /// lock it runs under; it runs only on the first seal.
    pub fn seal_with(&self, finish: impl FnOnce(Vec<T>) -> Vec<T>) -> &[T] {
        let mut open = self.open.relock();
        self.sealed
            .get_or_init(|| finish(std::mem::take(&mut *open)).into_boxed_slice())
    }

    /// Whether any item registered so far, sealed or not, matches `matches`.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader::engine::registry::Registry;
    ///
    /// let registry: Registry<u32> = Registry::new();
    /// registry.add(7)?;
    /// assert!(registry.any(|&item| item == 7));
    /// registry.seal();
    /// assert!(registry.any(|&item| item == 7));
    /// assert!(!registry.any(|&item| item == 8));
    /// # Ok::<(), enshwick_loader::engine::registry::Sealed>(())
    /// ```
    pub fn any(&self, matches: impl Fn(&T) -> bool) -> bool {
        let any = |items: &[T]| items.iter().any(&matches);
        match self.sealed() {
            Some(items) => any(items),
            // Sealed meanwhile: the sealed slice has every item.
            None => self
                .edit(|items| any(items))
                .unwrap_or_else(|Sealed| self.sealed().is_some_and(any)),
        }
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

    #[test]
    fn an_addition_while_the_seal_finishes_is_refused_not_kept_half_sealed() {
        static REGISTRY: Registry<u32> = Registry::new();
        assert_eq!(REGISTRY.add(1), Ok(()));
        let mut late = None;
        let sealed = REGISTRY.seal_with(|mut items| {
            late = Some(std::thread::spawn(|| REGISTRY.add(9)));
            items.push(2);
            items
        });
        assert_eq!(sealed, [1, 2]);
        let late = late.map(|thread| thread.join().ok());
        assert_eq!(
            late,
            Some(Some(Err(Sealed))),
            "the late addition is refused"
        );
        assert_eq!(REGISTRY.seal_with(|_| vec![3]), [1, 2], "finish runs once");
    }
}
