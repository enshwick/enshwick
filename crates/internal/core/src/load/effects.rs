//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::Load;
use crate::effects::Effects;
use enshwick_inventory::__load::*;

#[enshwick_macros::forward(base)]
impl Load<'_> {
    /// The mod's [`Effects`] in the server's world: game events it raises
    /// and spawns; on a client, the built-in server's while it hosts a
    /// game. An action whose event this build lacks answers
    /// [`WorldError::Missing`](crate::WorldError::Missing).
    ///
    /// # Examples
    ///
    /// See the [module](crate::effects).
    #[must_use]
    pub fn effects(&mut self) -> Effects;
}
