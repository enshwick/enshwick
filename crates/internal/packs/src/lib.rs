//! Internal part of [`enshwick`](https://crates.io/crates/enshwick): the
//! packs domain, data packs that describe content in TOML files instead of
//! code, which builds on every other domain.
//!
//! Mods depend on `enshwick`, which re-exports every module here at the
//! same path; this crate's own paths carry no promise.
#![cfg_attr(docsrs, feature(doc_cfg))]

// The lower crates' modules at the paths the code here names them by.
pub(crate) use enshwick_base::load::BaseLoad;
#[doc(hidden)]
pub use enshwick_base::{config, i18n, mods, points, schema_section, services};
pub(crate) use enshwick_base::{fresh, locks, near};
#[doc(hidden)]
pub use enshwick_content::content;
#[doc(hidden)]
pub use enshwick_inventory::{loot, survivors};
#[doc(hidden)]
pub use enshwick_knowledge::knowledge;
#[cfg(feature = "unstable")]
#[doc(hidden)]
pub use enshwick_knowledge::rewards;
#[doc(hidden)]
pub use enshwick_ui::sound;
#[doc(hidden)]
pub use enshwick_world::buffs;
// The macros the base exports, which the code here calls by `crate::`.
#[doc(hidden)]
pub use enshwick_base::{
    __every, __log, __off_from, __once, __require_unwind, __schema_entry, debug, error,
    error_every, error_once, info, info_every, info_once, warn, warn_every, warn_once,
};

/// The base handle's helpers at the path the code here names them by.
pub(crate) mod load {
    pub(crate) use enshwick_base::__off_from as off_from;
}

#[doc(inline)]
pub use enshwick_abi::abi;
pub mod packs;
#[doc(inline)]
pub use enshwick_values::shape;

#[doc(no_inline)]
pub use abi::{Entity, ItemId, PlayerKey, RecipeId};
pub use enshwick_base::{Host, Loading, Off};
#[doc(no_inline)]
pub use shape::Shape;

#[doc(hidden)]
pub mod __load;
/// What the macros' output names: the base's support.
#[doc(hidden)]
pub mod __private {
    pub use enshwick_base::__private::*;
}
