//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::Off;
use crate::WorldError;
use crate::abi::{Header, Status, WeatherFront};
use crate::weather::{FrontChanged, Weather};
use crate::{Host, Load};
use core::time::Duration;
use enshwick_world::__load::*;

#[cfg(feature = "typed")]
mod phenomena;

#[enshwick_macros::forward(base)]
impl Load<'_> {
    /// The [`Weather`] of the server's world; on a client, the built-in
    /// server's while it hosts a game.
    ///
    /// # Examples
    ///
    /// See the [module](crate::weather).
    #[must_use]
    pub fn weather(&mut self) -> Weather;
}

#[enshwick_macros::forward(base)]
#[crate::script(weather_fronts)]
impl Load<'_> {
    /// Calls `handler` when a front of the server's world starts and when
    /// it ends, about a second after, on the game thread; a mod gives the
    /// players inside its own weather a buff here. A panic in `handler`
    /// turns the mod off.
    ///
    /// # Errors
    ///
    /// [`Off`](crate::Off) after init.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::weather::WeatherPhenomenonEffect;
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     load.on_front(|change| {
    ///         if change.started && change.front.kind == WeatherPhenomenonEffect::Blizzard {
    ///             enshwick::info!("a blizzard of {} m starts", change.front.radius);
    ///         }
    ///     })
    /// }
    /// ```
    #[script]
    pub fn on_front<F>(&mut self, handler: F) -> Result<(), crate::Off>
    where
        F: Fn(&FrontChanged) + Send + Sync + 'static;
}
