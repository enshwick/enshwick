//! Internal part of [`enshwick`](https://crates.io/crates/enshwick): the
//! knowledge domain, what players know, quests and the journal, experience
//! and skills, and the rewards a mod gives.
//!
//! Mods depend on `enshwick`, which re-exports every module here at the
//! same path; this crate's own paths carry no promise.
#![cfg_attr(any(docsrs, enshwick_docs), feature(doc_cfg))]

// The macros name the API crate by its path, as mods see it; for the code
// here that crate is this one, whose root has the same paths.
extern crate self as enshwick;

// The lower crates' modules at the paths the code here names them by.
#[cfg(feature = "unstable")]
pub(crate) use enshwick_base::fresh;
pub(crate) use enshwick_base::load::BaseLoad;
#[cfg(feature = "unstable")]
pub(crate) use enshwick_base::locks;
#[doc(hidden)]
pub use enshwick_base::{events, game_events, resources, schedule, script, state};
#[doc(hidden)]
pub use enshwick_content::{content, rates};
#[cfg(feature = "unstable")]
#[doc(hidden)]
pub use enshwick_inventory::{inventory, items};
#[cfg(feature = "typed")]
#[doc(hidden)]
pub use enshwick_world::typed;
#[cfg(feature = "unstable")]
#[doc(hidden)]
pub use enshwick_world::world;
#[doc(hidden)]
pub use enshwick_world::{WorldError, buffs};
// The macros the base exports, which the code here calls by `crate::`.
#[doc(hidden)]
pub use enshwick_base::{
    __every, __log, __off_from, __once, __require_unwind, __schema_entry, debug, error,
    error_every, error_once, info, info_every, info_once, warn, warn_every, warn_once,
};

/// The base handle's helpers at the path the code here names them by.
pub(crate) mod load {
    pub(crate) use enshwick_base::load::hand_over;
}

#[doc(inline)]
pub use enshwick_abi::abi;
pub mod knowledge;
pub mod progress;
#[cfg(feature = "typed")]
pub mod quests;
#[doc(inline)]
pub use enshwick_abi::raw;
#[cfg(feature = "unstable")]
pub mod rewards;
#[doc(inline)]
pub use enshwick_values::shape;

#[doc(no_inline)]
pub use abi::{Entity, ItemId, PlayerKey, RecipeId};
pub use enshwick_base::{Host, Loading, Off, Relock, RelockRw, RelockWait, host};
// The macros the code here uses; `Script`, `call` and `script` come with the
// base's `script` module of the same name.
pub use enshwick_base::{Script, call};
pub use enshwick_macros::Shape;
#[doc(no_inline)]
pub use shape::Shape;

#[doc(hidden)]
pub mod __internal;
#[doc(hidden)]
pub mod __load;
/// What the macros' output names: the base's support.
#[doc(hidden)]
pub mod __private {
    pub use enshwick_base::__private::*;
}
