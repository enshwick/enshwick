//! The generated typed items the content domain reads, and the samples that
//! check their shapes; the handles that read them in a live world stay in
//! `enshwick-core`, which re-exports this module's items.

#[doc(inline)]
pub use enshwick_typed::typed::*;
pub use sample::sample;

mod sample;
