//! What mods register during init, open until the loader seals it and read
//! without a lock after.

use std::sync::Mutex;

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
pub struct Registry<T: 'static> {
    open: Mutex<Vec<T>>,
    sealed: SealedCell<T>,
}

/// The cell a shipped registry seals into; the loader's own tests seal into
/// [`ReopenableCell`] instead.
#[cfg(not(test))]
type SealedCell<T> = OnceCell<T>;
#[cfg(test)]
type SealedCell<T> = ReopenableCell<T>;

/// The sealed items: set once for the process.
#[derive(Debug)]
struct OnceCell<T>(std::sync::OnceLock<Box<[T]>>);

impl<T> OnceCell<T> {
    const fn new() -> Self {
        Self(std::sync::OnceLock::new())
    }

    #[inline]
    fn get(&self) -> Option<&[T]> {
        self.0.get().map(|items| &**items)
    }

    fn get_or_init(&self, init: impl FnOnce() -> Box<[T]>) -> &[T] {
        self.0.get_or_init(init)
    }
}

/// The sealed items in the loader's own tests: a test may open the registry
/// again for the next one, and what it sealed stays leaked for any reader
/// still holding it.
#[cfg(test)]
#[derive(Debug)]
struct ReopenableCell<T: 'static>(Mutex<Option<&'static [T]>>);

#[cfg(test)]
impl<T> ReopenableCell<T> {
    const fn new() -> Self {
        Self(Mutex::new(None))
    }

    fn get(&self) -> Option<&[T]> {
        *self.0.relock()
    }

    fn get_or_init(&self, init: impl FnOnce() -> Box<[T]>) -> &[T] {
        self.0.relock().get_or_insert_with(|| Box::leak(init()))
    }
}

impl<T> Registry<T> {
    /// An open, empty registry, for a `static`.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            open: Mutex::new(Vec::new()),
            sealed: SealedCell::new(),
        }
    }

    /// Opens the registry again, empty, so a test that sealed it leaves the
    /// next test an open one.
    #[cfg(test)]
    pub(crate) fn reopen(&self) {
        let mut open = self.open.relock();
        open.clear();
        *self.sealed.0.relock() = None;
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
        self.with(|items| items.iter().any(&matches))
    }

    /// Runs `read` on the items registered so far, sealed or not, as a mod's
    /// init asking about what it registered needs; `read` runs under the
    /// registry's lock while it is open, so it must not use this registry.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader::engine::registry::Registry;
    ///
    /// let registry: Registry<u32> = Registry::new();
    /// registry.add(7)?;
    /// assert_eq!(registry.with(<[u32]>::len), 1);
    /// registry.seal();
    /// assert_eq!(registry.with(<[u32]>::to_vec), [7]);
    /// # Ok::<(), enshwick_loader::engine::registry::Sealed>(())
    /// ```
    pub fn with<R>(&self, read: impl Fn(&[T]) -> R) -> R {
        match self.sealed() {
            Some(items) => read(items),
            // Sealed meanwhile: the sealed slice has every item.
            None => self
                .edit(|items| read(items))
                .unwrap_or_else(|Sealed| read(self.sealed().unwrap_or_default())),
        }
    }

    /// The sealed items; `None` while the registry is open.
    #[inline]
    #[must_use]
    pub fn sealed(&self) -> Option<&[T]> {
        self.sealed.get()
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

    #[test]
    fn the_shipped_cell_keeps_the_first_seal_and_ignores_later_ones() {
        let cell = OnceCell::new();
        assert_eq!(cell.get(), None);
        assert_eq!(cell.get_or_init(|| Box::new([1, 2])), [1, 2]);
        assert_eq!(cell.get_or_init(|| Box::new([3])), [1, 2]);
        assert_eq!(cell.get(), Some(&[1, 2][..]));
    }

    #[test]
    fn a_reopened_registry_takes_additions_again_and_starts_empty() {
        static REGISTRY: Registry<u32> = Registry::new();
        assert_eq!(REGISTRY.add(1), Ok(()));
        let first = REGISTRY.seal();
        REGISTRY.reopen();
        assert!(!REGISTRY.is_sealed());
        assert_eq!(REGISTRY.add(2), Ok(()));
        assert_eq!(REGISTRY.seal(), [2]);
        assert_eq!(first, [1], "a reader of the first seal keeps it");
    }
}
