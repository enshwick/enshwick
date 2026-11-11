//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::abi::Level;
use crate::content::{Guid, Id};
use crate::resources::Registry;
use crate::typed::{
    ResourceType, WeatherPhenomenonEffect as GameEffect, WeatherSystemPhenomenonResource,
    WeatherSystemResource,
};
use crate::weather::{NewPhenomenon, PhenomenonChange, PhenomenonId};
use crate::{Load, Off};
use core::time::Duration;
use enshwick_world::__load::*;

#[enshwick_macros::forward(base)]
#[crate::script(weather_phenomena)]
impl Load<'_> {
    /// Adds the weather phenomenon `new` to the game's weather, where the
    /// game picks it in its layer as it picks its own; mods that add
    /// phenomena apply in load order, each on the one before's result.
    ///
    /// # Errors
    ///
    /// [`Off`] after init, without the weather resource on this build, and
    /// for a phenomenon or layer the game's own weather lacks (a copy of
    /// another mod's phenomenon is refused), an id taken, a
    /// layer full, a value out of bounds or an effect the game does not
    /// know. A collision with another mod's phenomenon is logged and that
    /// mod's is kept.
    ///
    /// # Examples
    ///
    /// See [`NewPhenomenon`].
    #[script]
    pub fn add_weather_phenomenon(&mut self, new: NewPhenomenon) -> Result<PhenomenonId, Off>;

    /// Changes the values of the game's weather phenomenon `phenomenon`, or
    /// of one a mod added before; changes apply in load order.
    ///
    /// # Errors
    ///
    /// [`Off`] after init, without the weather resource on this build, and
    /// for a value out of bounds or an effect the game does not know. A
    /// phenomenon no mod before added and the game lacks is logged and
    /// left out.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::Guid;
    /// use enshwick::weather::PhenomenonChange;
    ///
    /// fn rarer(load: &mut enshwick::Load<'_>, rain: Guid) -> Result<(), enshwick::Off> {
    ///     load.change_weather_phenomenon(rain, PhenomenonChange::new().probability(0.1))
    /// }
    /// ```
    #[script]
    pub fn change_weather_phenomenon(
        &mut self,
        phenomenon: Guid,
        values: PhenomenonChange,
    ) -> Result<(), Off>;
}
