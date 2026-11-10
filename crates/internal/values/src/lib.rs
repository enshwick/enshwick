//! Internal part of [`enshwick`](https://crates.io/crates/enshwick): the
//! value types its layers share.
//!
//! Mods depend on `enshwick`, which re-exports every item here under its
//! own paths; this crate's own paths carry no promise.

// The macros name the API crate by its path, as mods see it; for the types
// here that crate is this one.
extern crate self as enshwick;

#[doc(hidden)]
pub mod abi;
pub mod content;
mod off;
#[doc(hidden)]
pub mod reason;
pub mod shape;

pub use enshwick_macros::Shape;
#[doc(hidden)]
pub use enshwick_macros::deps;
pub use off::Off;
pub use shape::Shape;

/// What the macros' output names.
#[doc(hidden)]
pub mod __private {
    pub use const_format::concatcp;
}
