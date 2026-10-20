//! Weather: a reading at the first placed entity, the fronts, rain started
//! and seen in the fronts, clear skies, and the game's own choice again;
//! chunk loads and unloads as they come.

use std::time::Duration;

use enshwick_loader_api::events::{ChunkLoaded, ChunkUnloaded};
use enshwick_loader_api::schedule::Scheduler;
use enshwick_loader_api::weather::{Forecast, FrontKind, Weather};
use enshwick_loader_api::world::World;
use enshwick_loader_api::{Load, Off};

/// How long the probe's rain lasts.
const RAIN: Duration = Duration::from_mins(2);
/// How long after a change the probe looks again.
const SETTLE: Duration = Duration::from_secs(1);
/// How long the checks take from their start.
pub(crate) const SPAN: Duration = SETTLE.saturating_mul(2);

/// The weather checks.
#[derive(Debug, Clone, Copy)]
pub(crate) struct WeatherChecks {
    weather: Weather,
    world: World,
    scheduler: Scheduler,
}

impl WeatherChecks {
    pub(crate) fn new(load: &mut Load<'_>, scheduler: Scheduler) -> Result<Self, Off> {
        load.on::<ChunkLoaded>(|chunk| {
            enshwick_loader_api::info!("event: chunk {},{} loaded", chunk.x, chunk.z);
        })?;
        load.on::<ChunkUnloaded>(|chunk| {
            enshwick_loader_api::info!("event: chunk {},{} unloaded", chunk.x, chunk.z);
        })?;
        Ok(Self {
            weather: load.weather()?,
            world: load.world()?,
            scheduler,
        })
    }

    pub(crate) fn run(self) {
        let at = self
            .world
            .entities(self.world.transform_type())
            .first()
            .map_or([0.0; 3], |placed| placed.position);
        if let Some(now) = self.weather.at(at) {
            enshwick_loader_api::info!("PASS weather at {at:?}: {:?} {now:?}", now.state());
        } else {
            enshwick_loader_api::warn!("FAIL weather: no reading");
            return;
        }
        enshwick_loader_api::info!("PASS weather fronts: {:?}", self.weather.fronts());
        if let Err(why) = self.weather.set(Forecast::Rain, Some(RAIN)) {
            enshwick_loader_api::warn!("FAIL weather rain: {why}");
            return;
        }
        self.scheduler.after(SETTLE, move || {
            let fronts = self.weather.fronts();
            if fronts.iter().any(|f| f.kind == FrontKind::Rain) {
                enshwick_loader_api::info!("PASS rain started: {fronts:?}");
            } else {
                enshwick_loader_api::warn!("FAIL rain started, fronts: {fronts:?}");
            }
            if let Err(why) = self.weather.set(Forecast::Clear, Some(RAIN)) {
                enshwick_loader_api::warn!("FAIL weather clear: {why}");
                return;
            }
            self.scheduler.after(SETTLE, move || {
                let fronts = self.weather.fronts();
                if fronts.iter().all(|f| f.kind == FrontKind::Clouds) {
                    enshwick_loader_api::info!("PASS clear skies: {fronts:?}");
                } else {
                    enshwick_loader_api::warn!("FAIL clear skies, fronts: {fronts:?}");
                }
                match self.weather.set(Forecast::Any, None) {
                    Ok(()) => enshwick_loader_api::info!("PASS weather back to the game's choice"),
                    Err(why) => enshwick_loader_api::warn!("FAIL weather back: {why}"),
                }
            });
        });
    }
}
