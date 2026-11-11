//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::game_settings::GameSettings;
use crate::{Load, Off};
use enshwick_world::__load::*;

#[enshwick_macros::forward(base)]
impl Load<'_> {
    /// The server's [`GameSettings`]; on a client, the built-in server's
    /// while it hosts a game.
    ///
    /// # Errors
    ///
    /// [`Off`] on a build whose settings lack a field this API knows.
    ///
    /// # Examples
    ///
    /// See the [module](crate::game_settings).
    pub fn game_settings(&mut self) -> Result<GameSettings, Off>;
}
