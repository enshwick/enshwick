//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::rates::{Rate, RateHandle};
use crate::{Load, Off};
use enshwick_content::__load::*;

#[cfg(feature = "unstable")]
#[enshwick_macros::forward(base)]
#[crate::script(rates)]
impl Load<'_> {
    /// Declares that this mod multiplies `rate` and hands back the handle
    /// that sets its factor, 1 until set. The loader scales the game's
    /// answers in every process the mod runs in; what the server decides,
    /// such as experience and loot, counts on the dedicated server and on a
    /// client while it hosts a game.
    ///
    /// # Errors
    ///
    /// [`Off`] on a build that lacks what the rate needs, and for a rate
    /// [`seen by clients`](Rate::seen_by_clients) in a mod without
    /// `join: true`; the rest of the mod keeps running when it carries on
    /// without this rate.
    ///
    /// # Examples
    ///
    /// See the [module](crate::rates).
    #[script]
    pub fn rate(&mut self, rate: Rate) -> Result<RateHandle, Off>;
}
