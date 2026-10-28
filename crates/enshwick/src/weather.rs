//! Weather in the server's world: how it is at a place, the fronts that
//! make it, and starting new weather.
//!
//! The game's weather comes in fronts: a few weather layers, each with a
//! current phenomenon at a place with a radius (rain, a blizzard, or clouds
//! without either) that lasts its time, after which the game picks the
//! next. Rain falls as snow where it is cold. [`Weather::at`] weighs a
//! place as the game's own systems do; [`Weather::set`] starts new fronts
//! the way the game's state configurations do, and every client follows.
//! How often rain and storms come is the server's `weatherFrequency` game
//! setting.
//!
//! Every call belongs on the server's game thread, from a game event, a
//! [`Scheduler`](crate::schedule::Scheduler) task or an action; on a client,
//! while it hosts a game.
//!
//! # Examples
//!
//! ```
//! use core::time::Duration;
//! use enshwick::weather::{Forecast, Weather};
//!
//! fn sun(weather: &Weather) -> Result<(), enshwick::WorldError> {
//!     weather.set(Forecast::Clear, Some(Duration::from_mins(10)))
//! }
//!
//! fn init(load: &mut enshwick::Load<'_>) -> Result<Weather, enshwick::Off> {
//!     Ok(load.weather())
//! }
//! ```

use core::time::Duration;

use crate::WorldError;
use crate::abi::{Header, Status, WeatherFront};
use crate::{Host, Load};

/// What the weather is at a place, as the game names it.
///
/// # Examples
///
/// See [`WeatherAt`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum WeatherState {
    /// Clear.
    Clear,
    /// Rain.
    Rain,
    /// Snow.
    Snow,
    /// A blizzard.
    Blizzard,
}

/// How the weather is at a place, from [`Weather::at`]: weights that add
/// up to 1, as the game's systems weigh it.
///
/// The name is enshwick's, not the game's.
///
/// # Examples
///
/// ```
/// use enshwick::weather::WeatherState;
///
/// fn wet(weather: &enshwick::weather::Weather, at: [f64; 3]) -> bool {
///     weather.at(at).is_some_and(|now| now.state() == WeatherState::Rain)
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct WeatherAt {
    /// How clear it is.
    pub clear: f32,
    /// How rainy.
    pub rain: f32,
    /// How snowy.
    pub snow: f32,
    /// How stormy.
    pub blizzard: f32,
}

impl WeatherAt {
    /// The strongest of the four.
    ///
    /// # Examples
    ///
    /// See [`WeatherAt`].
    #[must_use]
    pub fn state(&self) -> WeatherState {
        [
            (self.clear, WeatherState::Clear),
            (self.rain, WeatherState::Rain),
            (self.snow, WeatherState::Snow),
            (self.blizzard, WeatherState::Blizzard),
        ]
        .into_iter()
        .fold((f32::MIN, WeatherState::Clear), |best, next| {
            if next.0 > best.0 { next } else { best }
        })
        .1
    }
}

/// What a front brings.
///
/// # Examples
///
/// See [`Front`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum WeatherPhenomenonEffect {
    /// Clouds without rain.
    None,
    /// Rain, snow where it is cold.
    Rain,
    /// A blizzard.
    Blizzard,
    /// A kind this API does not name.
    Other(u32),
}

/// A front of weather, from [`Weather::fronts`].
///
/// # Examples
///
/// ```
/// use enshwick::weather::WeatherPhenomenonEffect;
///
/// fn storms(weather: &enshwick::weather::Weather) -> usize {
///     weather.fronts().iter().filter(|front| front.kind == WeatherPhenomenonEffect::Blizzard).count()
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct Front {
    /// The weather layer it belongs to.
    pub layer: u32,
    /// What it brings.
    pub kind: WeatherPhenomenonEffect,
    /// Where it is, in metres.
    pub position: [f64; 3],
    /// How far it reaches, in metres.
    pub radius: f32,
    /// How long it has left, in game time.
    pub remaining: Duration,
}

/// The weather to start, for [`Weather::set`].
///
/// The name is enshwick's, not the game's.
///
/// # Examples
///
/// See the [module](self).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Forecast {
    /// Clear skies everywhere.
    Clear,
    /// Rain fronts, snow where it is cold.
    Rain,
    /// Blizzards where the game has them.
    Blizzard,
    /// What the game picks itself, at once.
    Any,
}

/// The weather of the server's world; get it from [`Load::weather`].
///
/// # Examples
///
/// See the [module](self).
#[derive(Clone, Copy, Debug)]
pub struct Weather {
    host: Host,
}

impl Weather {
    /// How the weather is at `position`, in metres; `None` off the game
    /// thread or while no world runs.
    ///
    /// # Examples
    ///
    /// See [`WeatherAt`].
    #[must_use]
    pub fn at(&self, position: [f64; 3]) -> Option<WeatherAt> {
        let fixed = crate::ecs::fixed(position);
        let mut out = [0.0f32; 4];
        let status =
            (self.host.api().weather_at)(self.host.handle(), &raw const fixed, &raw mut out);
        (status == Status::OK).then_some(WeatherAt {
            clear: out[0],
            rain: out[1],
            snow: out[2],
            blizzard: out[3],
        })
    }

    /// The fronts now, one per weather layer that has one; empty off the
    /// game thread or while no world runs.
    ///
    /// # Examples
    ///
    /// See [`Front`].
    #[must_use]
    pub fn fronts(&self) -> Vec<Front> {
        let blank = WeatherFront {
            header: Header::of::<WeatherFront>(),
            layer: 0,
            effect: 0,
            position: [0; 3],
            radius: 0.0,
            reserved: 0,
            remaining: 0,
        };
        let mut fronts = vec![blank; 16];
        loop {
            let mut count = 0u32;
            let capacity = u32::try_from(fronts.len()).unwrap_or(u32::MAX);
            let status = (self.host.api().weather_fronts)(
                self.host.handle(),
                fronts.as_mut_ptr(),
                capacity,
                &raw mut count,
            );
            if status != Status::OK {
                return Vec::new();
            }
            let count = count as usize;
            if count <= fronts.len() {
                fronts.truncate(count);
                break;
            }
            fronts = vec![blank; count];
        }
        fronts
            .iter()
            .map(|front| Front {
                layer: front.layer,
                kind: match front.effect {
                    WeatherFront::NONE => WeatherPhenomenonEffect::None,
                    WeatherFront::RAIN => WeatherPhenomenonEffect::Rain,
                    WeatherFront::BLIZZARD => WeatherPhenomenonEffect::Blizzard,
                    other => WeatherPhenomenonEffect::Other(other),
                },
                position: crate::ecs::metres(front.position),
                radius: front.radius,
                remaining: Duration::from_nanos(u64::try_from(front.remaining).unwrap_or(0)),
            })
            .collect()
    }

    /// Starts new weather in every layer at once: fronts of the forecast
    /// where a layer has them and clear skies elsewhere, for `lasting` of
    /// game time or the game's own for `None`; [`Forecast::Any`] lets the
    /// game pick anew. The game places the fronts itself, as for its own.
    /// Clients follow, and the next save keeps it. Moving the clock forward
    /// with [`Clock::set_hours`](crate::daytime::Clock::set_hours) uses up
    /// the time the fronts have, and the game picks new ones.
    ///
    /// # Errors
    ///
    /// [`WorldError::WrongThread`] off the game thread and
    /// [`WorldError::Missing`] while no world runs or on a build without
    /// weather.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    pub fn set(&self, forecast: Forecast, lasting: Option<Duration>) -> Result<(), WorldError> {
        let effect = match forecast {
            Forecast::Clear => WeatherFront::NONE,
            Forecast::Rain => WeatherFront::RAIN,
            Forecast::Blizzard => WeatherFront::BLIZZARD,
            Forecast::Any => u32::MAX,
        };
        let duration = lasting.map_or(0, |d| {
            i64::try_from(d.as_nanos()).unwrap_or(i64::MAX).max(1)
        });
        WorldError::from_status((self.host.api().set_weather)(
            self.host.handle(),
            effect,
            duration,
        ))
    }
}

impl Load<'_> {
    /// The [`Weather`] of the server's world; on a client, the built-in
    /// server's while it hosts a game.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    #[must_use]
    pub fn weather(&mut self) -> Weather {
        Weather { host: self.host() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_strongest_weight_names_the_weather() {
        let now = WeatherAt {
            clear: 0.2,
            rain: 0.1,
            snow: 0.6,
            blizzard: 0.1,
        };
        assert_eq!(now.state(), WeatherState::Snow);
    }
}
