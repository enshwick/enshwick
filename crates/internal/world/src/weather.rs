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

#[cfg(feature = "typed")]
pub(crate) mod phenomena;

use core::time::Duration;

#[cfg(feature = "typed")]
pub use phenomena::{NewPhenomenon, PhenomenonChange, PhenomenonId};

use crate::__load::*;
use crate::Host;
use crate::WorldError;
use crate::abi::{Header, Status, WeatherFront};

/// What the weather is at a place, as the game names it.
///
/// # Examples
///
/// See [`WeatherAt`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, crate::Script)]
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
#[derive(Clone, Copy, Debug, PartialEq, crate::Script)]
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, crate::Script)]
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
#[derive(Clone, Copy, Debug, PartialEq, crate::Script)]
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
    /// The GUID of its phenomenon in the game's weather resource, which for
    /// one a mod added is the GUID its `PhenomenonId` holds; `None` when the
    /// loader does not tell.
    #[script(default)]
    pub phenomenon: Option<crate::content::Guid>,
}

/// The weather to start, for [`Weather::set`].
///
/// The name is enshwick's, not the game's.
///
/// # Examples
///
/// See the [module](self).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, crate::Script)]
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

/// The most time left a front may have when the world saves for the game to
/// restore it when the world loads again: 30 minutes of game time.
///
/// Measured on server build 26083076: fronts saved with up to 1795 s left
/// came back, and ones saved with 1825 s left or more did not.
///
/// # Examples
///
/// ```
/// use core::time::Duration;
/// use enshwick::weather::MAX_RESTORED_REMAINING;
///
/// assert_eq!(MAX_RESTORED_REMAINING, Duration::from_mins(30));
/// ```
pub const MAX_RESTORED_REMAINING: Duration = Duration::from_mins(30);

/// The weather of the server's world; get it from [`Load::weather`].
///
/// # Examples
///
/// See the [module](self).
#[derive(Clone, Copy, Debug)]
pub struct Weather {
    host: Host,
}

#[crate::script(sky)]
impl Weather {
    /// How the weather is at `position`, in metres; `None` off the game
    /// thread or while no world runs.
    ///
    /// # Examples
    ///
    /// See [`WeatherAt`].
    #[must_use]
    #[script]
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
    #[script]
    pub fn fronts(&self) -> Vec<Front> {
        let blank = WeatherFront {
            header: Header::of::<WeatherFront>(),
            layer: 0,
            effect: 0,
            position: [0; 3],
            radius: 0.0,
            reserved: 0,
            remaining: 0,
            phenomenon: [0; 16],
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
                phenomenon: (front
                    .header
                    .covers(core::mem::offset_of!(WeatherFront, phenomenon) + 16)
                    && front.phenomenon != [0; 16])
                    .then_some(crate::content::Guid(front.phenomenon)),
            })
            .collect()
    }

    /// Starts new weather in every layer at once: fronts of the forecast
    /// where a layer has them and clear skies elsewhere, for `lasting` of
    /// game time or the game's own for `None`; [`Forecast::Any`] lets the
    /// game pick anew. The game places the fronts itself, as for its own.
    /// Clients follow. A save keeps a front while it has at most
    /// [`MAX_RESTORED_REMAINING`] left: after a restart the game picks anew
    /// in a layer whose front had more, so a longer `lasting` logs a
    /// warning and lasts only until the world next loads. Moving the clock
    /// forward with [`Clock::set_hours`](crate::daytime::Clock::set_hours)
    /// uses up the time the fronts have, and the game picks new ones.
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
    #[script]
    pub fn set(&self, forecast: Forecast, lasting: Option<Duration>) -> Result<(), WorldError> {
        let effect = match forecast {
            Forecast::Clear => WeatherFront::NONE,
            Forecast::Rain => WeatherFront::RAIN,
            Forecast::Blizzard => WeatherFront::BLIZZARD,
            Forecast::Any => u32::MAX,
        };
        if let Some(lasting) = lasting.filter(|&lasting| lasting > MAX_RESTORED_REMAINING) {
            crate::warn!(
                "weather: fronts lasting {} s outlive no restart: a save keeps a front only while it has at most {} s left",
                lasting.as_secs(),
                MAX_RESTORED_REMAINING.as_secs()
            );
        }
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

/// `Load`'s methods of this module on the base handle, which core's
/// forwarders call.
#[doc(hidden)]
pub trait LoadMethods {
    /// As `Load::weather`.
    #[doc(hidden)]
    #[must_use]
    fn weather(&mut self) -> Weather;

    /// As `Load::on_front`.
    #[doc(hidden)]
    fn on_front<F>(&mut self, handler: F) -> Result<(), crate::Off>
    where
        F: Fn(&FrontChanged) + Send + Sync + 'static;
}

impl LoadMethods for crate::BaseLoad {
    fn weather(&mut self) -> Weather {
        Weather { host: self.host() }
    }

    fn on_front<F>(&mut self, handler: F) -> Result<(), crate::Off>
    where
        F: Fn(&FrontChanged) + Send + Sync + 'static,
    {
        let weather = self.weather();
        let mut before = Vec::new();
        self.scheduler()?.every(FRONT_POLL, move || {
            let now = weather.fronts();
            for change in front_changes(&before, &now) {
                handler(&change);
            }
            before = now;
            core::ops::ControlFlow::Continue(())
        });
        Ok(())
    }
}

/// A front that started or ended, for [`Load::on_front`].
///
/// # Examples
///
/// See [`Load::on_front`].
#[derive(Clone, Copy, Debug, PartialEq, crate::Script)]
#[non_exhaustive]
pub struct FrontChanged {
    /// The front as it started, or as it was last seen before it ended.
    pub front: Front,
    /// `true` when it started, `false` when it ended.
    pub started: bool,
}

/// How often [`Load::on_front`] looks at the fronts; a front lasts minutes.
const FRONT_POLL: Duration = Duration::from_secs(1);

/// Whether `now` is the front `before` was, a poll later: the same
/// phenomenon and effect, and no more time left than before.
fn same_front(before: &Front, now: &Front) -> bool {
    before.phenomenon == now.phenomenon
        && before.kind == now.kind
        && now.remaining <= before.remaining + FRONT_POLL
}

/// The fronts of `before` that ended by `now`, then those of `now` that
/// started since `before`, one front per layer.
fn front_changes(before: &[Front], now: &[Front]) -> Vec<FrontChanged> {
    let in_layer = |list: &[Front], layer| list.iter().find(|f| f.layer == layer).copied();
    let ended = before
        .iter()
        .filter(|b| in_layer(now, b.layer).is_none_or(|n| !same_front(b, &n)))
        .map(|&front| FrontChanged {
            front,
            started: false,
        });
    let started = now
        .iter()
        .filter(|n| in_layer(before, n.layer).is_none_or(|b| !same_front(&b, n)))
        .map(|&front| FrontChanged {
            front,
            started: true,
        });
    ended.chain(started).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn front(layer: u32, phenomenon: u8, remaining_secs: u64) -> Front {
        Front {
            layer,
            kind: WeatherPhenomenonEffect::Rain,
            position: [0.0; 3],
            radius: 1.0,
            remaining: Duration::from_secs(remaining_secs),
            phenomenon: Some(crate::content::Guid([phenomenon; 16])),
        }
    }

    #[test]
    fn a_front_that_runs_on_raises_nothing_and_a_new_one_ends_the_old_first() {
        let before = [front(0, 1, 60), front(1, 2, 30)];
        assert!(front_changes(&before, &[front(0, 1, 59), front(1, 2, 29)]).is_empty());
        assert!(
            front_changes(&before, &[front(0, 1, 60), front(1, 2, 30)]).is_empty(),
            "a front whose time stood still between two looks runs on"
        );
        let changes = front_changes(&before, &[front(0, 3, 90), front(1, 2, 29)]);
        assert_eq!(
            changes,
            [
                FrontChanged {
                    front: front(0, 1, 60),
                    started: false
                },
                FrontChanged {
                    front: front(0, 3, 90),
                    started: true
                },
            ]
        );
    }

    #[test]
    fn the_same_phenomenon_again_with_more_time_left_is_a_new_front() {
        let changes = front_changes(&[front(0, 1, 5)], &[front(0, 1, 120)]);
        assert_eq!(changes.len(), 2);
        assert!(!changes[0].started && changes[1].started);
    }

    #[test]
    fn a_layer_that_clears_ends_its_front_and_a_new_layer_starts_one() {
        let changes = front_changes(&[front(0, 1, 5)], &[front(2, 4, 50)]);
        assert_eq!(
            changes,
            [
                FrontChanged {
                    front: front(0, 1, 5),
                    started: false
                },
                FrontChanged {
                    front: front(2, 4, 50),
                    started: true
                },
            ]
        );
    }

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
