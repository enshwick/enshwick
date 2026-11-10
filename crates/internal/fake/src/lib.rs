//! Internal part of [`enshwick`](https://crates.io/crates/enshwick): the
//! fake loader for a mod's unit tests, which `enshwick` re-exports as
//! `enshwick::test` behind its `test` feature.
//!
//! Mods depend on `enshwick`; this crate's own paths carry no promise.
#![cfg_attr(docsrs, feature(doc_cfg))]

pub mod test;

// The examples here are written as a mod writes them, against `enshwick`.
// Each names this crate `enshwick`, so the root stands in for the API
// crate: the core's items at their paths beside `test`, which the macros'
// output names too.

#[doc(hidden)]
pub use enshwick_core::*;
