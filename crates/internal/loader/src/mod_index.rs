//! A loaded mod's place in load order, kept apart from every other number.

use enshwick::abi::ModHandle;

/// A loaded mod's place in load order, the index of its record. A mod's
/// [`ModHandle`] is its index plus one, and the loader itself owns links and
/// patches as [`ModIndex::LOADER`].
///
/// # Examples
///
/// ```
/// use enshwick_loader::mod_index::ModIndex;
///
/// let first = ModIndex::new(0);
/// assert_eq!(ModIndex::from_handle(first.handle()), Some(first));
/// assert!(ModIndex::LOADER.is_loader());
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ModIndex(usize);

impl ModIndex {
    /// The loader itself, as the owner of its own links, which never turn off.
    pub const LOADER: Self = Self(usize::MAX);

    /// The mod at `index` in load order.
    #[must_use]
    pub const fn new(index: usize) -> Self {
        Self(index)
    }

    /// The index into the mods in load order.
    #[must_use]
    pub const fn get(self) -> usize {
        self.0
    }

    /// Whether this is the loader rather than a mod.
    #[must_use]
    pub const fn is_loader(self) -> bool {
        self.0 == usize::MAX
    }

    /// The handle the mod is given: its index plus one, 0 being no mod.
    #[must_use]
    pub const fn handle(self) -> ModHandle {
        ModHandle(self.0.wrapping_add(1))
    }

    /// The mod a handle names; `None` for the handle of no mod.
    #[must_use]
    pub const fn from_handle(handle: ModHandle) -> Option<Self> {
        match handle.0.checked_sub(1) {
            Some(index) => Some(Self(index)),
            None => None,
        }
    }
}

impl core::fmt::Display for ModIndex {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if self.is_loader() {
            f.write_str("the loader")
        } else {
            write!(f, "mod #{}", self.0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_handle_is_the_index_plus_one_and_zero_is_no_mod() {
        assert_eq!(ModIndex::new(4).handle(), ModHandle(5));
        assert_eq!(ModIndex::from_handle(ModHandle(5)), Some(ModIndex::new(4)));
        assert_eq!(ModIndex::from_handle(ModHandle(0)), None);
    }
}
