//! The ids of game content: GUIDs, content ids and resource types.

use std::sync::OnceLock;

use crate::Off;

mod ids;
mod script;

#[doc(hidden)]
pub use enshwick_macros::id;
pub use ids::{DebugName, Guid, Id, NAMESPACE, is_id};
#[doc(hidden)]
pub use ids::{NAME_PART, part_name};

/// A type of game resource by enshwick's name for it, such as
/// [`types::ITEM_INFO`](types/constant.ITEM_INFO.html); the loader maps it to the running build's type,
/// whose hash resource keys carry.
///
/// # Examples
///
/// ```
/// # #[cfg(any())] fn in_enshwick() { // names items above this crate: enshwick's doctests compile it
/// use enshwick::content::types;
///
/// assert_eq!(types::ITEM_INFO.id(), "item_info");
/// assert!(types::ITEM_INFO.hash().is_err(), "outside the game");
/// # }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ResourceType(&'static str);

impl ResourceType {
    /// The type the loader's catalogue names `id`.
    #[doc(hidden)]
    #[must_use]
    pub const fn new(id: &'static str) -> Self {
        Self(id)
    }

    /// Its enshwick id, which the loader's catalogue maps to the engine.
    ///
    /// # Examples
    ///
    /// See [`ResourceType`].
    #[must_use]
    pub const fn id(self) -> &'static str {
        self.0
    }

    /// Its hash on the running build.
    ///
    /// # Errors
    ///
    /// [`Off`] outside the game or when the build lacks the type.
    ///
    /// # Examples
    ///
    /// See [`ResourceType`].
    pub fn hash(self) -> Result<u32, Off> {
        TYPE_HASHES
            .get()
            .and_then(|hashes| hashes(self.0))
            .unwrap_or_else(|| Err(Off::new(format!("type {}: outside the game", self.0))))
    }
}

/// How [`ResourceType::hash`] asks the running build: the hash of the type
/// with an id, or `None` outside the game.
#[doc(hidden)]
pub type TypeHashes = fn(&'static str) -> Option<Result<u32, Off>>;

// The loader's table sits in the API crate, above this one, so that crate
// hands its lookup down once, as a logger is set; the first one stays.
static TYPE_HASHES: OnceLock<TypeHashes> = OnceLock::new();

/// Answers [`ResourceType::hash`] with `hashes` from now on; the first call
/// wins.
#[doc(hidden)]
pub fn answer_type_hashes(hashes: TypeHashes) {
    TYPE_HASHES.get_or_init(|| hashes);
}
