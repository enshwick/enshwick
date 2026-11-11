//! Internal part of [`enshwick`](https://crates.io/crates/enshwick): the
//! fake loader for a mod's unit tests, which `enshwick` re-exports as
//! `enshwick::test` behind its `test` feature.
//!
//! Mods depend on `enshwick`; this crate's own paths carry no promise.
#![cfg_attr(docsrs, feature(doc_cfg))]

pub mod test;

// The root stands in for the API crate: core's items at their paths beside
// `test`. The fake's own code names them through `crate::`, the examples
// name this crate `enshwick` as a mod writes them, and so does the macros'
// output.

#[doc(hidden)]
pub use enshwick_core::*;
