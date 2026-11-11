//! Internal part of [`enshwick`](https://crates.io/crates/enshwick): every
//! domain of the API not yet in a crate of its own, and [`Load`] on top,
//! whose methods forward into the crates below.
//!
//! Mods depend on `enshwick`, which re-exports every module here at the
//! same path; this crate's own paths carry no promise.
#![cfg_attr(docsrs, feature(doc_cfg))]

// The macros name the API crate by its path, as mods see it; for the code
// and examples here that crate is this one, whose root has the same paths.
extern crate self as enshwick;

// The base's modules at the paths the domains here name them by.
#[doc(hidden)]
pub use enshwick_base::{
    code, command_names,
    config::{self},
    ecs, events, files, game_events, i18n, mods, net, pacing, points, resources, schedule,
    services, state, status, time,
};
pub(crate) use enshwick_base::{
    declare, engine_enum, engine_ids, facts, fresh,
    hook::{self},
    host, locks, near, rules,
};
// The macros the base exports, which the code here calls by `crate::`.
#[doc(hidden)]
pub use enshwick_base::{
    __declare_exports, __every, __log, __off_from, __once, __require_unwind, __schema_entry, debug,
    detour, error, error_every, error_once, info, info_every, info_once, warn, warn_every,
    warn_once,
};

#[doc(inline)]
pub use enshwick_abi::abi;
#[cfg(feature = "unstable")]
pub mod actions;
pub mod affixes;
pub mod altars;
pub mod buffs;
pub mod chat;
#[cfg(feature = "unstable")]
pub mod commands;
#[cfg(feature = "unstable")]
pub mod console;
pub mod content;
#[cfg(feature = "unstable")]
pub mod controls;
pub mod damage;
pub mod daytime;
pub mod effects;
#[cfg(feature = "unstable")]
pub mod entity_data;
pub mod game_settings;
#[cfg(feature = "unstable")]
pub mod input;
pub mod interactions;
pub mod inventory;
#[cfg(feature = "unstable")]
pub mod item_data;
#[cfg(feature = "unstable")]
pub mod items;
// Without `unstable` item data stays inside, for the stats of single items.
#[cfg(not(feature = "unstable"))]
#[expect(
    unreachable_pub,
    dead_code,
    reason = "item data is public only with `unstable`"
)]
mod item_data;
pub mod knowledge;
mod load;
pub mod loot;
#[cfg(feature = "unstable")]
pub mod map;
pub mod packs;
pub mod progress;
#[cfg(feature = "typed")]
pub mod quests;
#[cfg(feature = "unstable")]
pub mod rates;
// Without `unstable` the rates stay inside, for the recipe and comfort
// scaling the stable parts and the fake host share.
#[cfg(not(feature = "unstable"))]
#[expect(
    unreachable_pub,
    dead_code,
    reason = "the rates are public only with `unstable`"
)]
mod rates;
#[doc(inline)]
pub use enshwick_abi::raw;
#[cfg(feature = "unstable")]
pub mod rewards;
#[cfg(feature = "unstable")]
pub mod script;
// Without `unstable` the binding stays inside, so the crate's own types keep
// their derives; only a script runtime, which turns the feature on, reads
// them.
#[cfg(not(feature = "unstable"))]
#[expect(
    unreachable_pub,
    dead_code,
    reason = "the script binding is public only with `unstable`"
)]
mod script;
#[doc(inline)]
pub use enshwick_values::shape;
pub mod sound;
pub mod stats;
pub mod survivors;
pub mod terrain;
#[cfg(feature = "typed")]
pub mod typed;
pub mod ui;
pub mod vetoes;
pub mod voxels;
pub mod weather;
#[cfg(feature = "unstable")]
pub mod world;
mod world_error;

#[doc(no_inline)]
pub use abi::{Entity, ItemId, PlayerKey, RecipeId};
pub use enshwick_base::{FnPtr, Hook, Host, Loading, Next, Relock, RelockRw, RelockWait, host};
// The macros, documented where mods meet them, on `enshwick`'s root.
pub use enshwick_macros::{
    Config, Shape, config, declare_mod, deps, event, hook, point, service, texts,
};
#[cfg(feature = "unstable")]
pub use enshwick_macros::{Script, call, script};
#[cfg(not(feature = "unstable"))]
pub(crate) use enshwick_macros::{Script, script};
pub use load::{Load, Mod, Off};
#[doc(no_inline)]
pub use shape::Shape;
pub use world_error::WorldError;

#[cfg(feature = "fake")]
#[doc(hidden)]
pub mod __fake;
#[doc(hidden)]
pub mod __private;
#[doc(hidden)]
pub use enshwick_base::schema_section;
