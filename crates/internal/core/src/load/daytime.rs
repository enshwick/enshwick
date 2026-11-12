//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::daytime::{Clock, ClockSpeed};
use crate::{Load, Off};
use enshwick_world::__load::*;

#[enshwick_macros::forward(base)]
#[crate::script(daytime)]
impl Load<'_> {
    /// The [`Clock`] of the server's world; on a client, the built-in
    /// server's while it hosts a game.
    ///
    /// # Examples
    ///
    /// See the [module](crate::daytime).
    #[must_use]
    pub fn clock(&mut self) -> Clock;

    /// The mod's [`ClockSpeed`]: its factor on how fast the server's clock
    /// runs. The loader follows the game's speed for it from the start.
    ///
    /// # Errors
    ///
    /// [`Off`] after init or on a build without the clock's speed.
    ///
    /// # Examples
    ///
    /// See [`ClockSpeed`].
    #[script]
    pub fn clock_speed(&mut self) -> Result<ClockSpeed, Off>;
}
