//! Internal part of [`enshwick`](https://crates.io/crates/enshwick): what
//! every domain of the API shares, below the domains and the `Load` they
//! hang from.
//!
//! Mods depend on `enshwick`, which re-exports every module here at the
//! same path; this crate's own paths carry no promise.
#![cfg_attr(any(docsrs, enshwick_docs), feature(doc_cfg))]

/// Fails when rustdoc builds this crate's examples one by one, which it does
/// without a word when one of them does not merge.
///
/// ```
/// assert_ne!(module_path!(), "rust_out", "an example does not merge: \
///     RUSTC_BOOTSTRAP=1 RUSTDOCFLAGS='-Zunstable-options --merge-doctests=yes' \
///     cargo test --doc names it");
/// ```
#[cfg(doctest)]
struct MergedExamples;

// The macros name the API crate by its path, as mods see it; for the code
// here that crate is this one, whose root has the same paths.
extern crate self as enshwick;

#[doc(inline)]
pub use enshwick_abi::abi;
pub mod code;
#[doc(hidden)]
pub mod command_names;
pub mod config;
// The content ids, which the macros and the base name here; the content
// domain's module joins them with the rest.
#[doc(hidden)]
pub use enshwick_values::content;
#[doc(hidden)]
pub mod declare;
pub mod ecs;
#[doc(hidden)]
pub mod engine_enum;
#[doc(hidden)]
pub mod engine_ids;
pub mod events;
#[doc(hidden)]
pub mod facts;
pub mod files;
#[doc(hidden)]
pub mod fresh;
pub mod game_events;
#[doc(hidden)]
pub mod hook;
#[doc(hidden)]
pub mod host;
pub mod i18n;
#[doc(hidden)]
pub mod load;
#[doc(hidden)]
pub mod locks;
pub mod mods;
#[doc(hidden)]
pub mod near;
pub mod net;
pub mod pacing;
pub mod points;
#[doc(inline)]
pub use enshwick_abi::raw;
pub mod resources;
#[doc(hidden)]
pub mod rules;
pub mod schedule;
#[doc(hidden)]
pub mod script;
pub mod services;
#[doc(inline)]
pub use enshwick_values::shape;
pub mod state;
pub mod status;
pub mod time;

#[doc(no_inline)]
pub use abi::{Entity, ItemId, PlayerKey, RecipeId};
// The macros the code here uses, beside the ones its own types derive.
pub use enshwick_macros::{Config, Shape, config, event, hook, point, service, texts};
pub use enshwick_macros::{Script, call, script};
pub use hook::{FnPtr, Hook, Next};
pub use host::{Host, host};
pub(crate) use load::BaseLoad;
pub use load::{Loading, Off};
pub use locks::{Relock, RelockRw, RelockWait};
#[doc(no_inline)]
pub use shape::Shape;

#[cfg(feature = "fake")]
#[doc(hidden)]
pub mod __fake;
#[doc(hidden)]
pub mod __internal;
#[doc(hidden)]
pub mod __private;
#[doc(hidden)]
pub mod peer;
#[doc(hidden)]
pub mod schema_section;
