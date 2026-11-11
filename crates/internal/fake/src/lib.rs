//! Internal part of [`enshwick`](https://crates.io/crates/enshwick): the
//! fake loader for a mod's unit tests, which `enshwick` re-exports as
//! `enshwick::test` behind its `test` feature.
//!
//! Mods depend on `enshwick`; this crate's own paths carry no promise.
#![cfg_attr(docsrs, feature(doc_cfg))]

pub mod test;
