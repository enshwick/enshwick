//! Internal part of [`enshwick`](https://crates.io/crates/enshwick): the ui
//! domain, the client's interface drawn with the game's own widgets, and
//! the game's sounds.
//!
//! Mods depend on `enshwick`, which re-exports every module here at the
//! same path; this crate's own paths carry no promise.
#![cfg_attr(any(docsrs, enshwick_docs), feature(doc_cfg))]

// The macros name the API crate by its path, as mods see it; for the code
// here that crate is this one, whose root has the same paths.
extern crate self as enshwick;

// The lower crates' modules at the paths the code here names them by.
#[cfg(feature = "unstable")]
pub(crate) use enshwick_base::engine_ids;
pub(crate) use enshwick_base::load::BaseLoad;
pub(crate) use enshwick_base::locks;
#[doc(hidden)]
pub use enshwick_base::{ecs, mods, resources, script};
#[doc(hidden)]
pub use enshwick_content::content;
#[cfg(feature = "typed")]
#[doc(hidden)]
pub use enshwick_world::typed;
#[cfg(feature = "unstable")]
#[doc(hidden)]
pub use enshwick_world::{map, world};
// The macros the base exports, which the code here calls by `crate::`.
#[doc(hidden)]
pub use enshwick_base::{
    __every, __log, __off_from, __once, __require_unwind, __schema_entry, debug, error,
    error_every, error_once, info, info_every, info_once, warn, warn_every, warn_once,
};

/// The base handle's helpers at the path the code here names them by.
pub(crate) mod load {
    pub(crate) use enshwick_base::__off_from as off_from;
    pub(crate) use enshwick_base::load::{hand_over, ok};
}

#[doc(inline)]
pub use enshwick_abi::abi;
#[doc(inline)]
pub use enshwick_abi::raw;
#[doc(inline)]
pub use enshwick_values::shape;
pub mod sound;
pub mod ui;

#[doc(no_inline)]
pub use abi::{Entity, ItemId, PlayerKey, RecipeId};
pub use enshwick_base::{Host, Loading, Off, host};
// The macros the code here uses; `Script`, `call` and `script` come with the
// base's `script` module of the same name.
pub use enshwick_base::{Script, call};
pub use enshwick_macros::Shape;
#[doc(no_inline)]
pub use shape::Shape;

#[cfg(feature = "fake")]
#[doc(hidden)]
pub mod __fake;
#[doc(hidden)]
pub mod __internal;
#[doc(hidden)]
pub mod __load;
/// What the macros' output names: the base's support.
#[doc(hidden)]
pub mod __private {
    pub use enshwick_base::__private::*;
}
