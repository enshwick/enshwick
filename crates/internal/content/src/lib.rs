//! Internal part of [`enshwick`](https://crates.io/crates/enshwick): the
//! content domain, the game's items, recipes, workshops and the rest a mod
//! adds or changes, with the rates that scale them and the generated typed
//! items they read.
//!
//! Mods depend on `enshwick`, which re-exports every module here at the
//! same path; this crate's own paths carry no promise.
#![cfg_attr(docsrs, feature(doc_cfg))]
#![forbid(unsafe_code)]

// The macros name the API crate by its path, as mods see it; for the code
// here that crate is this one, whose root has the same paths.
extern crate self as enshwick;

// The base's modules at the paths the code here names them by.
#[doc(hidden)]
pub use enshwick_base::{
    code, command_names,
    config::{self},
    ecs, events, files, game_events, i18n, mods, net, pacing, points, resources, schedule, script,
    services, state, status, time,
};
pub(crate) use enshwick_base::{engine_enum, engine_ids, facts, fresh, locks, near};
// The macros the base exports, which the code here calls by `crate::`.
pub(crate) use enshwick_base::load::BaseLoad;
#[doc(hidden)]
pub use enshwick_base::{
    __every, __log, __off_from, __once, __require_unwind, __schema_entry, debug, detour, error,
    error_every, error_once, info, info_every, info_once, warn, warn_every, warn_once,
};

/// The base handle's helpers at the path the code here names them by.
pub(crate) mod load {
    pub(crate) use enshwick_base::__off_from as off_from;
    pub(crate) use enshwick_base::load::{game_events_of, off_status, ok};
}

#[doc(inline)]
pub use enshwick_abi::abi;
pub mod content;
#[doc(hidden)]
pub mod knowledge;
#[doc(hidden)]
pub mod rates;
#[doc(inline)]
pub use enshwick_abi::raw;
#[doc(inline)]
pub use enshwick_values::shape;
#[cfg(feature = "typed")]
#[doc(hidden)]
pub mod typed;

#[doc(no_inline)]
pub use abi::{Entity, ItemId, PlayerKey, RecipeId};
pub use enshwick_base::{
    FnPtr, Hook, Host, Loading, Next, Off, Relock, RelockRw, RelockWait, host,
};
// The macros the code here uses; `Script`, `call` and `script` come with the
// base's `script` module of the same name.
pub use enshwick_base::{Script, call};
pub use enshwick_macros::{Config, Shape, config, event, hook, point, service, texts};
#[doc(no_inline)]
pub use shape::Shape;

#[doc(hidden)]
pub mod __load;
#[doc(hidden)]
pub use enshwick_base::__private;
