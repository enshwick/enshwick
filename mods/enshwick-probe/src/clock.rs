//! The clock: a read, a time across the turn of day set and read back, a factor on the speed and
//! the turns of day and night.

use std::time::Duration;

use enshwick_loader_api::daytime::{Clock, ClockSpeed};
use enshwick_loader_api::events::{DayStarted, NightStarted};
use enshwick_loader_api::schedule::Scheduler;
use enshwick_loader_api::{Load, Off};

/// How long after a change the probe reads the clock again.
const SETTLE: Duration = Duration::from_secs(2);
/// The factor the probe sets on the clock's speed.
const FACTOR: f32 = 10.0;

/// The clock checks.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ClockChecks {
    clock: Clock,
    speed: ClockSpeed,
    scheduler: Scheduler,
}

impl ClockChecks {
    pub(crate) fn new(load: &mut Load<'_>, scheduler: Scheduler) -> Result<Self, Off> {
        load.on::<DayStarted>(|()| enshwick_loader_api::info!("event: day started"))?;
        load.on::<NightStarted>(|()| enshwick_loader_api::info!("event: night started"))?;
        Ok(Self {
            clock: load.clock()?,
            speed: load.clock_speed()?,
            scheduler,
        })
    }

    /// Reads the clock, moves it across the turn of day or night, reads it
    /// back, puts it back,
    /// then speeds it up and checks the speed.
    pub(crate) fn run(self) {
        let Some(before) = self.clock.now() else {
            enshwick_loader_api::warn!("FAIL clock: no reading");
            return;
        };
        enshwick_loader_api::info!("PASS clock reads {before:?}");
        // Across the turn, so the day or the night event follows.
        let target = if before.is_day {
            before.night_starts + 1.0
        } else {
            before.day_starts + 1.0
        } % 24.0;
        if let Err(why) = self.clock.set_hours(target) {
            enshwick_loader_api::warn!("FAIL clock set: {why}");
            return;
        }
        self.scheduler.after(SETTLE, move || {
            match self.clock.now() {
                Some(now) if (now.hours - target).abs() < 0.25 => {
                    enshwick_loader_api::info!(
                        "PASS clock set to {target:.2} h reads {:.2} h",
                        now.hours
                    );
                }
                other => {
                    enshwick_loader_api::warn!("FAIL clock set to {target:.2} h reads {other:?}");
                }
            }
            let _back = self.clock.set_hours(before.hours);
            self.speed_up(before.speed);
        });
    }

    fn speed_up(self, game: f32) {
        if let Err(why) = self.speed.set(FACTOR) {
            enshwick_loader_api::warn!("FAIL clock speed: {why}");
            return;
        }
        self.scheduler.after(SETTLE, move || {
            match self.clock.now() {
                Some(now) if (now.speed - game * FACTOR).abs() <= game * FACTOR * 0.01 => {
                    enshwick_loader_api::info!(
                        "PASS clock speed {FACTOR} times {game} reads {}",
                        now.speed
                    );
                }
                other => enshwick_loader_api::warn!(
                    "FAIL clock speed {FACTOR} times {game} reads {other:?}"
                ),
            }
            let _back = self.speed.set(1.0);
            self.scheduler
                .after(SETTLE, move || match self.clock.now() {
                    Some(now) if (now.speed - game).abs() <= game * 0.01 => {
                        enshwick_loader_api::info!("PASS clock speed back to {}", now.speed);
                    }
                    other => enshwick_loader_api::warn!("FAIL clock speed back reads {other:?}"),
                });
        });
    }
}
