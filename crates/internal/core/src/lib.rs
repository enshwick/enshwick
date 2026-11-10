//! Internal part of [`enshwick`](https://crates.io/crates/enshwick): the safe
//! layer over the ABI and every domain of the API, from [`Load`] to the
//! fake host.
//!
//! Mods depend on `enshwick`, which re-exports every module here at the
//! same path; this crate's own paths carry no promise.
#![cfg_attr(docsrs, feature(doc_cfg))]

/// The version of Enshwick this crate is part of, released at one version
/// with `enshwick`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

// The macros name the API crate by its path, as mods see it; for the code
// and examples here that crate is this one, whose root has the same paths.
extern crate self as enshwick;

#[doc(inline)]
pub use enshwick_abi::abi;
#[cfg(feature = "unstable")]
pub mod actions;
pub mod affixes;
pub mod altars;
pub mod buffs;
pub mod chat;
pub mod code;
#[doc(hidden)]
pub mod command_names;
#[cfg(feature = "unstable")]
pub mod commands;
pub mod config;
#[cfg(feature = "unstable")]
pub mod console;
pub mod content;
#[cfg(feature = "unstable")]
pub mod controls;
pub mod damage;
pub mod daytime;
mod declare;
pub mod ecs;
pub mod effects;
mod engine_enum;
mod engine_ids;
#[cfg(feature = "unstable")]
pub mod entity_data;
pub mod events;
pub mod files;
pub(crate) mod fresh;
pub mod game_events;
pub mod game_settings;
mod hook;
mod host;
pub mod i18n;
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
mod locks;
pub mod loot;
#[cfg(feature = "unstable")]
pub mod map;
pub mod mods;
mod near;
pub mod net;
pub mod pacing;
pub mod packs;
pub mod points;
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
pub mod resources;
#[cfg(feature = "unstable")]
pub mod rewards;
mod rules;
pub mod schedule;
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
pub mod services;
#[doc(inline)]
pub use enshwick_values::shape;
pub mod sound;
pub mod state;
pub mod stats;
pub mod status;
pub mod survivors;
pub mod terrain;
#[cfg(feature = "test")]
pub mod test;
pub mod time;
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
// The macros, documented where mods meet them, on `enshwick`'s root; `config`
// and `script` are documented here, since `enshwick` takes each with its
// module of the same name.
/// Declares a mod's settings struct: serde's `Deserialize` and `Serialize`,
/// unknown keys refused and [`trait@config::Config`], so the mod needs no
/// serde of its own.
///
/// A key the file leaves out reads from `Default`. `file = "name"`,
/// `validate = path` and `text = "key"` are the struct attributes of
/// [`trait@config::Config`]; field attributes stay `#[config(..)]` and
/// `#[serde(..)]`.
///
/// # Examples
///
/// ```
/// # extern crate enshwick_core as enshwick;
/// use enshwick::config::Config;
///
/// /// Locker settings.
/// #[enshwick::config(file = "lockers")]
/// #[derive(Default)]
/// struct Settings {
///     /// How many lockers one player may own.
///     #[config(range = 1..=16)]
///     limit: u32,
/// }
///
/// assert!(Settings::template().contains("limit = 0"));
/// assert_eq!(Settings::FILE, "lockers");
/// ```
///
/// A misspelt key is refused with its name, not read past:
///
/// ```
/// # extern crate enshwick_core as enshwick;
/// /// Locker settings.
/// #[enshwick::config]
/// #[derive(Debug, Default)]
/// struct Settings {
///     /// How many lockers one player may own.
///     limit: u32,
/// }
///
/// let error = toml::from_str::<Settings>("limits = 4").map_err(|e| e.to_string());
/// assert!(error.is_err_and(|e| e.contains("unknown field `limits`")));
/// ```
pub use enshwick_macros::config;
/// Exports the methods of an `impl` block marked `#[script]` to script
/// languages: `#[script(<part>)]` adds the constant `SCRIPT_<PART>`, a list
/// of [`script::Function`] a [`script::Bound`] type names in its parts.
///
/// Unstable: needs the `unstable` feature and may change in any release.
///
/// A method takes `self` or `&self` and has a doc comment. Parameters and
/// the return type implement [`script::FromScript`] and
/// [`script::IntoScript`]; `&T` reads a `T`; an `impl Fn(A) -> R` parameter
/// takes a script function, with `A: IntoScript` and `R: script::Answer`; a
/// `Result` returns its `Ok` value and raises its `Err`'s message.
///
/// # Examples
///
/// See [`script::Function`].
#[cfg(feature = "unstable")]
pub use enshwick_macros::script;
pub use enshwick_macros::{Config, Shape, declare_mod, deps, event, hook, point, service, texts};
#[cfg(feature = "unstable")]
pub use enshwick_macros::{Script, call};
#[cfg(not(feature = "unstable"))]
pub(crate) use enshwick_macros::{Script, script};
pub use hook::{FnPtr, Hook, Next};
pub use host::{Host, host};
pub use load::{Load, Mod, Off};
pub use locks::{Relock, RelockRw, RelockWait};
#[doc(no_inline)]
pub use shape::Shape;
pub use world_error::WorldError;

#[doc(hidden)]
pub mod __private;
#[doc(hidden)]
pub mod schema_section;
