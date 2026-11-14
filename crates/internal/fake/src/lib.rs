//! Internal part of [`enshwick`](https://crates.io/crates/enshwick): the
//! fake loader for a mod's unit tests, which `enshwick` re-exports as
//! `enshwick::test` behind its `test` feature.
//!
//! Mods depend on `enshwick`; this crate's own paths carry no promise.
#![cfg_attr(docsrs, feature(doc_cfg))]

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

pub mod test;
