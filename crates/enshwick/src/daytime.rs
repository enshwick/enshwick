//! The clock of the server's world: the time of day, setting it, how fast
//! it runs, and the turns of day and night.
//!
//! The game's clock runs in game hours: on the 1.0 game the day lasts from
//! 06:00 to 22:00 and takes 30 real minutes, the night 12 by default. Players sleeping at
//! night make it run faster. A time a mod sets reaches every client after
//! the next step; the clients jump to it once and run on from there.
//! [`DayStarted`](crate::events::DayStarted) and
//! [`NightStarted`](crate::events::NightStarted) mark the turns.
//!
//! Every call belongs on the server's game thread, from a game event, a
//! [`Scheduler`](crate::schedule::Scheduler) task or an action; on a client,
//! while it hosts a game.
//!
//! # Examples
//!
//! ```
//! use enshwick::actions::{Action, Outcome};
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     let clock = load.clock();
//!     let actions = load.actions(enshwick::i18n::Texts::default())?;
//!     actions.add(Action::new("noon", "Set the time to noon").admins(), move |_request| {
//!         match clock.set_hours(12.0) {
//!             Ok(()) => Outcome::done("It is noon"),
//!             Err(why) => Outcome::refused(format!("No noon: {why}")),
//!         }
//!     })
//! }
//! ```

use core::time::Duration;

use crate::abi::{DaytimeInfo, Header, Status};
use crate::world::WorldError;
use crate::{Host, Load, Off};

/// Game nanoseconds in an hour.
const HOUR: f64 = 3_600_000_000_000.0;

/// The clock as it was when read, from [`Clock::now`].
///
/// The name is enshwick's; its day and night lengths are the game settings'
/// fields of the same names.
///
/// # Examples
///
/// ```
/// fn report(clock: &enshwick::daytime::Clock) -> String {
///     match clock.now() {
///         Some(now) if now.is_day => format!("{:.1} h, day", now.hours),
///         Some(now) => format!("{:.1} h, night", now.hours),
///         None => "no world".into(),
///     }
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct TimeOfDay {
    /// The time of day in hours, from 0 up to 24.
    pub hours: f32,
    /// Whether it is day.
    pub is_day: bool,
    /// The hour the day starts at.
    pub day_starts: f32,
    /// The hour the night starts at.
    pub night_starts: f32,
    /// How long the day lasts in real time at speed 1.
    pub day_time_duration: Duration,
    /// How long the night lasts in real time at speed 1.
    pub night_time_duration: Duration,
    /// How fast the clock runs now: 1 normally, faster while players sleep,
    /// times every mod's factor.
    pub speed: f32,
}

impl TimeOfDay {
    fn from_info(info: &DaytimeInfo) -> Self {
        let length = |ns: i64| Duration::from_nanos(u64::try_from(ns).unwrap_or(0));
        Self {
            hours: hours(info.time),
            is_day: info.flags & DaytimeInfo::DAY != 0,
            day_starts: hours(info.day_start),
            night_starts: hours(info.day_end),
            day_time_duration: length(info.day_length),
            night_time_duration: length(info.night_length),
            speed: info.speed,
        }
    }
}

#[expect(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    reason = "a time of day in hours needs no more than an f32 holds"
)]
fn hours(ns: i64) -> f32 {
    (ns as f64 / HOUR) as f32
}

/// The clock of the server's world; get it from [`Load::clock`].
///
/// # Examples
///
/// See the [module](self).
#[derive(Clone, Copy, Debug)]
pub struct Clock {
    host: Host,
}

impl Clock {
    /// The clock now; `None` off the game thread or while no world runs.
    ///
    /// # Examples
    ///
    /// See [`TimeOfDay`].
    #[must_use]
    pub fn now(&self) -> Option<TimeOfDay> {
        let mut info = DaytimeInfo {
            header: Header::of::<DaytimeInfo>(),
            time: 0,
            day_start: 0,
            day_end: 0,
            day_length: 0,
            night_length: 0,
            speed: 0.0,
            flags: 0,
        };
        let status = (self.host.api().time_of_day)(self.host.handle(), &raw mut info);
        (status == Status::OK).then(|| TimeOfDay::from_info(&info))
    }

    /// Sets the time of day to `hours`, taken modulo 24; every client jumps
    /// to it after the next step. The world moves forward to the next time
    /// the clock shows that hour, as if the hours between had passed.
    ///
    /// # Errors
    ///
    /// [`WorldError::Refused`] for an hour that is not a finite number,
    /// [`WorldError::WrongThread`] off the game thread and
    /// [`WorldError::Missing`] on a build without the clock.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    pub fn set_hours(&self, hours: f32) -> Result<(), WorldError> {
        if !hours.is_finite() {
            return Err(WorldError::Refused);
        }
        #[expect(
            clippy::cast_possible_truncation,
            reason = "the hours are taken modulo a day first"
        )]
        let ns = (f64::from(hours).rem_euclid(24.0) * HOUR) as i64;
        WorldError::from_status((self.host.api().set_time_of_day)(self.host.handle(), ns))
    }
}

/// The mod's say in how fast the server's clock runs; get it from
/// [`Load::clock_speed`].
///
/// The clock runs at the game's own speed times every mod's factor, so mods
/// compose: one that doubles the day and one that halves it leave it as it
/// was. Players sleeping at night still speed it up on top.
///
/// # Examples
///
/// ```
/// use enshwick::actions::{Action, Outcome};
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     let speed = load.clock_speed()?;
///     let actions = load.actions(enshwick::i18n::Texts::default())?;
///     actions.add(Action::new("slow-time", "Halve the clock's speed").admins(), move |_request| {
///         Outcome::done(format!("{:?}", speed.set(0.5)))
///     })
/// }
/// ```
#[derive(Clone, Copy, Debug)]
pub struct ClockSpeed {
    host: Host,
}

impl ClockSpeed {
    /// The largest factor the loader takes.
    pub const MAX: f32 = 1000.0;

    /// Sets the mod's factor on the clock's speed: 1 leaves it, 2 makes the
    /// days pass twice as fast, 0 stops the clock. Clients follow after the
    /// next step.
    ///
    /// # Errors
    ///
    /// [`WorldError::Refused`] for a factor outside 0 to [`ClockSpeed::MAX`],
    /// [`WorldError::WrongThread`] off the game thread and
    /// [`WorldError::Missing`] on a build without the clock.
    ///
    /// # Examples
    ///
    /// See [`ClockSpeed`].
    pub fn set(&self, factor: f32) -> Result<(), WorldError> {
        if !(0.0..=Self::MAX).contains(&factor) {
            return Err(WorldError::Refused);
        }
        WorldError::from_status((self.host.api().set_time_speed)(self.host.handle(), factor))
    }
}

impl Load<'_> {
    /// The [`Clock`] of the server's world; on a client, the built-in
    /// server's while it hosts a game.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    #[must_use]
    pub fn clock(&mut self) -> Clock {
        Clock { host: self.host() }
    }

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
    pub fn clock_speed(&mut self) -> Result<ClockSpeed, Off> {
        let host = self.host();
        let status = (host.api().time_speed_control)(host.handle());
        crate::load::ok(status, ClockSpeed { host }, || "clock speed".to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nanoseconds_read_as_hours() {
        assert!((hours(18 * 3_600_000_000_000) - 18.0).abs() < 1e-6);
        assert!((hours(5_400_000_000_000) - 1.5).abs() < 1e-6);
    }
}
