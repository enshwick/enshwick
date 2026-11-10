//! The handles that resolve a generated component, event or resource on the
//! running build and check its shape, over what the generated items' own
//! crate holds for them.

mod handles;
mod outside;
mod sample;
pub(crate) mod script;

pub(crate) use enshwick_typed::typed::runtime::*;
pub use handles::{ComponentType, EventType, ResourceType};
pub(crate) use handles::{RawComponent, RawEvent, RawResource};
pub use sample::sample;

#[cfg(test)]
mod tests;
