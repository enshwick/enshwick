//! A game resource's data as a tree of typed values.

#[doc(hidden)]
pub mod script;
mod value;

pub use value::{Value, Variant};
