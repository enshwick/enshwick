//! Internal part of [`enshwick`](https://crates.io/crates/enshwick): every
//! component, event and resource the game reflects, as typed Rust items
//! generated from the game's own reflection.
//!
//! Mods depend on `enshwick`, which re-exports every item here as
//! `enshwick::typed`; this crate's own paths carry no promise.

#![forbid(unsafe_code)]

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

pub mod typed;

// The generated items and their macros name these by the API crate's paths.
#[doc(hidden)]
pub use enshwick_values::{__private, Off, content, resources, script};
