//! Internal part of [`enshwick`](https://crates.io/crates/enshwick): the
//! actions domain, the actions players run from the interface, console
//! commands, controls and the players' input, chat, interactions and vetoes.
//!
//! Mods depend on `enshwick`, which re-exports every module here at the
//! same path; this crate's own paths carry no promise.
#![cfg_attr(docsrs, feature(doc_cfg))]

// The macros name the API crate by its path, as mods see it; for the code
// here that crate is this one, whose root has the same paths.
extern crate self as enshwick;

// The lower crates' modules at the paths the code here names them by.
pub(crate) use enshwick_base::load::BaseLoad;
#[doc(hidden)]
pub use enshwick_base::{
    code, command_names,
    config::{self},
    ecs, events, files, game_events, i18n, mods, net, pacing, points, resources, schedule, script,
    services, state, status, time,
};
#[cfg(feature = "unstable")]
pub(crate) use enshwick_base::{engine_ids, locks, near};
#[doc(hidden)]
pub use enshwick_content::{content, rates};
#[doc(hidden)]
pub use enshwick_world::WorldError;
#[cfg(feature = "unstable")]
#[doc(hidden)]
pub use enshwick_world::world;
// The macros the base exports, which the code here calls by `crate::`.
#[doc(hidden)]
pub use enshwick_base::{
    __every, __log, __off_from, __once, __require_unwind, __schema_entry, debug, detour, error,
    error_every, error_once, info, info_every, info_once, warn, warn_every, warn_once,
};

/// The base handle's helpers at the path the code here names them by.
pub(crate) mod load {
    #[cfg(feature = "unstable")]
    pub(crate) use enshwick_base::__off_from as off_from;
    pub(crate) use enshwick_base::load::hand_over;
    #[cfg(feature = "unstable")]
    pub(crate) use enshwick_base::load::ok;
}

#[doc(inline)]
pub use enshwick_abi::abi;
#[cfg(feature = "unstable")]
pub mod actions;
pub mod chat;
#[cfg(feature = "unstable")]
pub mod commands;
#[cfg(feature = "unstable")]
pub mod console;
#[cfg(feature = "unstable")]
pub mod controls;
#[cfg(feature = "unstable")]
pub mod input;
pub mod interactions;
#[doc(inline)]
pub use enshwick_abi::raw;
#[doc(inline)]
pub use enshwick_values::shape;
pub mod vetoes;

#[doc(no_inline)]
pub use abi::{Entity, ItemId, PlayerKey, RecipeId};
pub use enshwick_base::{
    FnPtr, Hook, Host, Loading, Next, Off, Relock, RelockRw, RelockWait, host,
};
// The macros the code here uses; `Script`, `call` and `script` come with the
// base's `script` module of the same name.
pub use enshwick_base::{Script, call};
pub use enshwick_macros::{Config, Shape, config, deps, event, hook, point, service, texts};
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
