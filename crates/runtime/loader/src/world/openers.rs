//! The mods that asked for a part of the world during init, which only they
//! may then use.

use crate::engine::mod_index::ModIndex;

use crate::engine::registry::{Registry, Sealed};

/// The mods that opened a part of the world, each once, in the order they
/// asked; sealed when the part links.
#[derive(Debug, Default)]
pub(crate) struct Openers(Registry<ModIndex>);

impl Openers {
    /// No mod yet, for a domain's state.
    pub(crate) const fn new() -> Self {
        Self(Registry::new())
    }

    /// Adds mod `owner`; a second ask of the same mod adds nothing.
    pub(crate) fn open(&self, owner: ModIndex) -> Result<(), Sealed> {
        self.0.edit(|openers| {
            if !openers.contains(&owner) {
                openers.push(owner);
            }
        })
    }

    /// Whether mod `owner` opened the part.
    pub(crate) fn opened(&self, owner: ModIndex) -> bool {
        self.0.any(|&opener| opener == owner)
    }

    /// Seals the openers and returns them; later calls return the same.
    pub(crate) fn seal(&self) -> &[ModIndex] {
        self.0.seal()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mod_opens_once_and_none_opens_after_the_seal() {
        let openers = Openers::new();
        let (first, second) = (ModIndex::new(1), ModIndex::new(2));
        assert_eq!(openers.open(first), Ok(()));
        assert_eq!(openers.open(first), Ok(()));
        assert!(openers.opened(first));
        assert!(!openers.opened(second));
        assert_eq!(openers.seal(), [first]);
        assert_eq!(openers.open(second), Err(Sealed));
        assert!(openers.opened(first), "read from the sealed list");
        assert!(!openers.opened(second));
    }
}
