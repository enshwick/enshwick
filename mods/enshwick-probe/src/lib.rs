//! Development only, never shipped: exercises the platform API on a test
//! server without a player and logs what it sees to
//! `logs/enshwick/mods/probe.log`, each check as a `PASS` or `FAIL` line.
//!
//! Off until `enabled = true` in `mod-configs/probe.toml`; read at start.
//! While on, it counts every engine event by type and logs the counts every
//! minute, logs each typed engine event it receives, and runs its checks
//! once the world has loaded: world key, entity queries, stats, a per-world
//! store and a save on demand. `/probe` commands run them again, from the
//! admin tools' command file as well.

mod census;
mod checks;

use std::ops::ControlFlow;
use std::sync::Arc;
use std::time::Duration;

use enshwick_loader_api::events::{WorldLoaded, WorldSaved};
use enshwick_loader_api::game_events::{
    AltarAction, Built, CraftRequested, Crafted, Died, EquipmentChanged, Hit, Interacted, LevelUp,
    LootOpened, PickedUp, Revived, TornDown, Typed, XpGained,
};
use enshwick_loader_api::{Load, Mod, Off};

use crate::census::Census;
use crate::checks::Checks;

/// How often the census is logged.
const CENSUS_PERIOD: Duration = Duration::from_secs(60);
/// How long after the world loads the checks run.
const CHECK_DELAY: Duration = Duration::from_secs(10);

/// Probe settings, read at start.
#[derive(Debug, Default, serde::Deserialize, serde::Serialize, enshwick_loader_api::Config)]
#[serde(deny_unknown_fields)]
struct Settings {
    /// Exercise the platform API and log what it sees; development only.
    enabled: bool,
}

/// The probe.
#[derive(Debug)]
pub struct Probe;

/// Logs every event of kind `E`, or that its fields are missing.
fn log_typed<E: Typed + core::fmt::Debug>(load: &mut Load<'_>) {
    match load.on_game::<E>(|event| enshwick_loader_api::info!("event: {event:?}")) {
        Ok(()) => enshwick_loader_api::info!("PASS typed {} resolves", E::TYPE_NAME),
        Err(off) => enshwick_loader_api::warn!("FAIL typed {}: {off}", E::TYPE_NAME),
    }
}

impl Mod for Probe {
    fn manifest() -> &'static enshwick_loader_api::abi::Manifest {
        enshwick_loader_api::manifest!(id: "probe", side: SERVER)
    }

    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        let settings: Settings = load.config()?;
        if !settings.enabled {
            return Ok(Self);
        }
        let census = Arc::new(Census::new(load));
        let counted = Arc::clone(&census);
        load.on_every_game_event(move |event| counted.count(event))?;
        log_typed::<Hit>(load);
        log_typed::<Died>(load);
        log_typed::<LevelUp>(load);
        log_typed::<XpGained>(load);
        log_typed::<CraftRequested>(load);
        log_typed::<Crafted>(load);
        log_typed::<Interacted>(load);
        log_typed::<Built>(load);
        log_typed::<TornDown>(load);
        log_typed::<PickedUp>(load);
        log_typed::<AltarAction>(load);
        log_typed::<LootOpened>(load);
        log_typed::<EquipmentChanged>(load);
        log_typed::<Revived>(load);

        let checks = Arc::new(Checks::new(load)?);
        let scheduler = load.scheduler()?;
        let logged = Arc::clone(&census);
        scheduler.every(CENSUS_PERIOD, move || {
            for line in logged.lines() {
                enshwick_loader_api::info!("census: {line}");
            }
            ControlFlow::Continue(())
        });
        let first = Arc::clone(&checks);
        load.on::<WorldLoaded>(move |()| {
            let checks = Arc::clone(&first);
            scheduler.after(CHECK_DELAY, move || checks.run());
        })?;
        let saved = Arc::clone(&checks);
        load.on::<WorldSaved>(move |()| saved.saved())?;
        load.command(
            "probe",
            "development probe; /probe census|check|vitals <id>|destroy <id>",
            move |_ctx, args| match args {
                ["census"] => census.lines().join("\n").into(),
                ["check"] => {
                    checks.run();
                    "checks run; see probe.log".into()
                }
                ["vitals", id] => checks.vitals_of(id).into(),
                ["destroy", id] => checks.destroy(id).into(),
                _ => "/probe census|check|vitals <id>|destroy <id>".into(),
            },
        )?;
        enshwick_loader_api::info!("probe on");
        Ok(Self)
    }
}

enshwick_loader_api::declare_mod!(Probe);
