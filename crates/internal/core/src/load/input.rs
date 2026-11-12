//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::input::Input;
use crate::{Load, Off};
use enshwick_actions::__load::*;

#[enshwick_macros::forward(base)]
impl Load<'_> {
    /// The players' held game controls in the game server, dedicated or
    /// hosted by the client.
    ///
    /// Unstable: needs the `unstable` feature and may change in any release.
    ///
    /// # Errors
    ///
    /// [`Off`] when the game build lacks the input reads.
    ///
    /// # Examples
    ///
    /// See [`Input`].
    pub fn input(&mut self) -> Result<Input, Off>;
}
