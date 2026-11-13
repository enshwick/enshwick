//! Internal part of [`enshwick`](https://crates.io/crates/enshwick): the
//! binary interface between the loader and a mod, and the reads and writes
//! of game memory.
//!
//! Mods depend on `enshwick`, which re-exports the modules here as
//! `enshwick::abi` and `enshwick::raw`; this crate's own paths carry no
//! promise.

#![cfg_attr(enshwick_docs, feature(doc_cfg))]

// The macros name the API crate by its path, as mods see it; for the types
// here that crate is this one.
extern crate self as enshwick;

pub mod abi;
pub mod raw;

// The ids the API crate has at its root, where the examples here name them.
#[doc(hidden)]
pub use abi::{Entity, ItemId, PlayerKey, RecipeId};
// The derives on the ABI types name these by the API crate's paths.
#[doc(hidden)]
pub use enshwick_values::{__private, Script, Shape, content, script, shape};

/// The version of Enshwick the ABI's messages and the loader name: the API
/// crate's, which is released at this crate's version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
