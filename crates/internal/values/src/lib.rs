//! Internal part of [`enshwick`](https://crates.io/crates/enshwick): the
//! value types its layers share.
//!
//! Mods depend on `enshwick`, which re-exports every item here under its
//! own paths; this crate's own paths carry no promise.

pub mod shape;

pub use enshwick_macros::Shape;
pub use shape::Shape;

/// What the macros' output names.
#[doc(hidden)]
pub mod __private {
    pub use const_format::concatcp;
}
