//! Every component, event and resource the game reflects, as typed Rust
//! items generated from the game's own reflection.
//!
//! A module per reflected namespace holds its types, read and written
//! by field name, so a type whose shape a build changes turns off alone.
//!
//! A game update needs no change for a moved field or a renumbered
//! constant; a removed or retyped field turns off only its type at
//! resolve, naming the build; a type renamed with the same layout
//! resolves under its new name. The book's chapter "Typed types across
//! game builds" explains each case.
//!
//! # Examples
//!
//! ```
//! # extern crate enshwick_core as enshwick;
//! use enshwick::typed::{ComponentType, ecs::Health};
//!
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     let health = ComponentType::<Health>::resolve(load)?;
//!     let world = load.world()?;
//!     load.on::<enshwick::events::PlayerJoined>(move |joined| {
//!         if let Ok(Some(value)) = health.read(world, joined.entity) {
//!             enshwick::info!("{value:?}");
//!         }
//!     })
//! }
//! ```

#[expect(
    hidden_glob_reexports,
    reason = "the handles' module stands over the generated items' own, which it re-exports"
)]
pub(crate) mod runtime;

#[doc(inline)]
pub use enshwick_typed::typed::*;
#[doc(hidden)]
pub use runtime::sample;
pub use runtime::{ComponentType, EventType, ResourceType};
