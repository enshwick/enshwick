//! Internal part of [`enshwick`](https://crates.io/crates/enshwick): every
//! component, event and resource the game reflects, as typed Rust items
//! generated from the game's own reflection.
//!
//! Mods depend on `enshwick`, which re-exports every item here as
//! `enshwick::typed`; this crate's own paths carry no promise.

#![forbid(unsafe_code)]

pub mod typed;

// The generated items and their macros name these by the API crate's paths.
#[doc(hidden)]
pub use enshwick_values::{__private, Off, content, resources, script};
