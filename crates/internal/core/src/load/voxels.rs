//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::Load;
use crate::voxels::Voxels;
use enshwick_world::__load::*;

mod edits;

#[enshwick_macros::forward(base)]
impl Load<'_> {
    /// The [`Voxels`] of the server's world; on a client, the built-in
    /// server's while it hosts a game.
    ///
    /// # Examples
    ///
    /// See the [module](crate::voxels).
    #[must_use]
    pub fn voxels(&mut self) -> Voxels;
}
