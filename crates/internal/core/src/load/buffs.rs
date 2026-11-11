//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::Load;
use crate::buffs::Buffs;
use enshwick_world::__load::*;

#[enshwick_macros::forward(base)]
impl Load<'_> {
    /// The [`Buffs`] of the server's world; on a client, the built-in
    /// server's while it hosts a game.
    ///
    /// # Examples
    ///
    /// See the [module](crate::buffs).
    #[must_use]
    pub fn buffs(&mut self) -> Buffs;
}
