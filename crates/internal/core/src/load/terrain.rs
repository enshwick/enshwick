//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::Load;
use crate::terrain::Terrain;
use enshwick_world::__load::*;

#[enshwick_macros::forward(base)]
impl Load<'_> {
    /// The [`Terrain`] of the server's world; on a client, the built-in
    /// server's while it hosts a game.
    ///
    /// # Examples
    ///
    /// See the [module](crate::terrain).
    #[must_use]
    pub fn terrain(&mut self) -> Terrain;
}
