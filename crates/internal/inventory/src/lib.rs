//! Internal part of [`enshwick`](https://crates.io/crates/enshwick): the
//! inventory domain, the inventories of the server's world and the items in
//! them, the data items and entities carry, affixes and stats, damage and
//! status effects, survivors and loot.
//!
//! Mods depend on `enshwick`, which re-exports every module here at the
//! same path; this crate's own paths carry no promise.
#![cfg_attr(any(docsrs, enshwick_docs), feature(doc_cfg))]

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
pub(crate) use enshwick_base::{engine_enum, engine_ids, fresh, locks};
#[doc(hidden)]
pub use enshwick_content::{content, rates};
#[doc(hidden)]
pub use enshwick_world::{WorldError, altars, buffs};
// The macros the base exports, which the code here calls by `crate::`.
#[doc(hidden)]
pub use enshwick_base::{
    __every, __log, __off_from, __once, __require_unwind, __schema_entry, debug, detour, error,
    error_every, error_once, info, info_every, info_once, warn, warn_every, warn_once,
};

/// The base handle's helpers at the path the code here names them by.
pub(crate) mod load {
    pub(crate) use enshwick_base::__off_from as off_from;
    pub(crate) use enshwick_base::load::{hand_over, ok};
}

#[doc(inline)]
pub use enshwick_abi::abi;
pub mod affixes;
pub mod damage;
pub mod effects;
#[cfg(feature = "unstable")]
pub mod entity_data;
pub mod inventory;
#[cfg(feature = "unstable")]
pub mod item_data;
// Without `unstable` item data stays inside, for the stats of single items.
#[cfg(not(feature = "unstable"))]
#[expect(
    unreachable_pub,
    dead_code,
    reason = "item data is public only with `unstable`"
)]
mod item_data;
#[cfg(feature = "unstable")]
pub mod items;
pub mod loot;
#[doc(inline)]
pub use enshwick_abi::raw;
#[doc(inline)]
pub use enshwick_values::shape;
pub mod stats;
pub mod survivors;

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
