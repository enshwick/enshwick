//! Development only, never shipped: exercises the platform API on a test
//! server without a player and logs what it sees to
//! `logs/enshwick/mods/probe.log`, each check as a `PASS`, `FAIL` or `SKIP` line.
//!
//! Off until `enabled = true` in `mod-configs/probe.toml`; read at start.
//! While on, it counts every game event by type and logs the counts every
//! minute, logs each typed game event it receives, and runs its checks
//! once the world has loaded: world key, entity queries, stats, component
//! writes, actions, spawns, a per-world store and a save on demand. `/probe` commands run them again, from the
//! admin tools' command file as well.

mod census;
mod checks;
mod clock;
mod settings;

use std::ops::ControlFlow;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use enshwick_loader_api::events::{WorldLoaded, WorldSaved};
use enshwick_loader_api::game_events::{
    AltarUsed, Built, CraftRequested, Crafted, Died, EquipmentChanged, ExperienceGained, Hit,
    Interacted, Kind, LevelGained, LootOpened, PickedUp, Revived, TornDown,
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
#[serde(default, deny_unknown_fields)]
struct Settings {
    /// Exercise the platform API and log what it sees; development only.
    enabled: bool,
    /// Refuse every craft request, to see the craft filter in game.
    refuse_crafting: bool,
    /// Refuse every placement, to see the build filter in game.
    refuse_building: bool,
}

/// The probe.
#[derive(Debug)]
pub struct Probe;

/// Logs every event of kind `E`, or that its fields are missing.
fn log_typed<E: Kind + core::fmt::Debug>(load: &mut Load<'_>) {
    match load.on_game::<E>(|event| enshwick_loader_api::info!("event: {event:?}")) {
        Ok(()) => enshwick_loader_api::info!("PASS typed {} resolves", E::TYPE_NAME),
        Err(off) => enshwick_loader_api::warn!("FAIL typed {}: {off}", E::TYPE_NAME),
    }
}

/// Logs the craft requests and the first placement the filters see, and
/// refuses them when the settings say so.
fn vetoes(load: &mut Load<'_>, settings: &Settings) {
    let refuse_crafting = settings.refuse_crafting;
    let crafts = load.on_craft(move |craft| {
        enshwick_loader_api::info!("craft filter: {craft:?}, refused: {refuse_crafting}");
        !refuse_crafting
    });
    match crafts {
        Ok(()) => enshwick_loader_api::info!("PASS craft filter registered"),
        Err(off) => enshwick_loader_api::warn!("FAIL craft filter: {off}"),
    }
    let refuse_building = settings.refuse_building;
    let seen = AtomicBool::new(false);
    let builds = load.on_build(move |placement| {
        if !seen.swap(true, Ordering::Relaxed) {
            enshwick_loader_api::info!(
                "build filter, first call: {placement:?}, refused: {refuse_building}"
            );
        }
        !refuse_building
    });
    match builds {
        Ok(()) => enshwick_loader_api::info!("PASS build filter registered"),
        Err(off) => enshwick_loader_api::warn!("FAIL build filter: {off}"),
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
        load.on_every_raw_game_event(move |event| counted.count(event))?;
        log_typed::<Hit>(load);
        log_typed::<Died>(load);
        log_typed::<LevelGained>(load);
        log_typed::<ExperienceGained>(load);
        log_typed::<CraftRequested>(load);
        log_typed::<Crafted>(load);
        log_typed::<Interacted>(load);
        log_typed::<Built>(load);
        log_typed::<TornDown>(load);
        log_typed::<PickedUp>(load);
        log_typed::<AltarUsed>(load);
        log_typed::<LootOpened>(load);
        log_typed::<EquipmentChanged>(load);
        log_typed::<Revived>(load);

        vetoes(load, &settings);
        // The probe is no join mod, so the loader refuses it a channel.
        match load.channel::<u32, _>("probe", 1, |_, _| ()) {
            Err(off) => enshwick_loader_api::info!(
                "PASS a channel is refused to a mod that is no join mod: {off}"
            ),
            Ok(_) => {
                enshwick_loader_api::warn!("FAIL a channel opened for a mod that is no join mod");
            }
        }
        let checks = Arc::new(Checks::new(load)?);
        let ping = load.event_type("keen::ecs::UiPingEvent")?;
        let pinged = Arc::clone(&checks);
        load.on_raw_game_event(ping, move |event| pinged.pinged(event))?;
        let scheduler = load.scheduler()?;
        let logged = Arc::clone(&census);
        scheduler.every(CENSUS_PERIOD, move || {
            for line in logged.lines() {
                enshwick_loader_api::info!("census: {line}");
            }
            ControlFlow::Continue(())
        });
        // Checks of single features, each run after the general ones.
        let mut extras: Vec<Box<dyn Fn() + Send + Sync>> = Vec::new();
        match clock::ClockChecks::new(load, scheduler) {
            Ok(clock) => extras.push(Box::new(move || clock.run())),
            Err(off) => enshwick_loader_api::warn!("FAIL clock: {off}"),
        }
        match settings::SettingsChecks::new(load) {
            Ok(settings) => extras.push(Box::new(move || settings.run())),
            Err(off) => enshwick_loader_api::warn!("FAIL game settings: {off}"),
        }
        let extras: Arc<[Box<dyn Fn() + Send + Sync>]> = extras.into();
        let run_all = {
            let checks = Arc::clone(&checks);
            move || {
                checks.run();
                for extra in extras.iter() {
                    extra();
                }
            }
        };
        let run_all = Arc::new(run_all);
        let first = Arc::clone(&run_all);
        load.on::<WorldLoaded>(move |()| {
            let run = Arc::clone(&first);
            scheduler.after(CHECK_DELAY, move || run());
        })?;
        let saved = Arc::clone(&checks);
        load.on::<WorldSaved>(move |()| saved.saved())?;
        load.add_command(
            "probe",
            "development probe; /probe census|check|vitals <id>|destroy <id>|spawn <id>|notice <text>",
            move |ctx, args| match args {
                _ if !ctx.is_admin() => "/probe is for administrators".into(),
                ["census"] => census.lines().join("\n").into(),
                ["check"] => {
                    run_all();
                    "checks run; see probe.log".into()
                }
                ["vitals", id] => checks.vitals_of(id).into(),
                ["destroy", id] => checks.destroy(id).into(),
                ["spawn", id] => checks.spawn_copy(id).into(),
                ["notice", words @ ..] => checks.notice(&ctx.player, &words.join(" ")).into(),
                _ => "/probe census|check|vitals <id>|destroy <id>|spawn <id>|notice <text>".into(),
            },
        )?;
        enshwick_loader_api::info!("probe on");
        Ok(Self)
    }
}

enshwick_loader_api::declare_mod!(Probe);
