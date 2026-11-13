//! Internal part of [`enshwick`](https://crates.io/crates/enshwick): the
//! value types its layers share.
//!
//! Mods depend on `enshwick`, which re-exports every item here under its
//! own paths; this crate's own paths carry no promise.

#![cfg_attr(enshwick_docs, feature(doc_cfg))]

// The macros name the API crate by its path, as mods see it; for the types
// here that crate is this one.
extern crate self as enshwick;

#[doc(hidden)]
pub mod abi;
pub mod content;
mod off;
#[doc(hidden)]
pub mod reason;
pub mod resources;
pub mod script;
pub mod shape;

#[doc(hidden)]
pub use enshwick_macros::deps;
pub use enshwick_macros::{Script, Shape, script};
pub use off::Off;
pub use shape::Shape;

/// What the macros' output names.
#[doc(hidden)]
pub mod __private {
    pub use const_format::{concatcp, str_replace};

    pub use crate::script::convert::{argument, argument_or_default, record_fields, taken, text};

    /// A copy of `this`, for a `#[script]` method that takes `self` by value.
    #[must_use]
    pub fn owned<T: Clone>(this: &T) -> T {
        this.clone()
    }
}
