//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::world::World;
use crate::{Load, Off};
use enshwick_world::__load::*;

mod fields;
mod join;
mod rename;

#[enshwick_macros::forward(base)]
impl Load<'_> {
    /// The server's [`World`]: reads, queries and requests; on a client,
    /// the built-in server's while it hosts a game.
    ///
    /// Unstable: needs the `unstable` feature and may change in any release.
    ///
    /// # Errors
    ///
    /// [`Off`] when the game build lacks a type the reads need.
    ///
    /// # Examples
    ///
    /// See [`World`].
    pub fn world(&mut self) -> Result<World, Off>;
}
