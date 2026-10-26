//! Development only, never shipped: exercises the platform API on a test
//! server without a player and logs what it sees to
//! `logs/enshwick/mods/probe.log`, each check as a `PASS`, `FAIL` or `SKIP` line.
//!
//! Off until `enabled = true` in `mod-configs/probe.toml`; read at start.
//! While on, it counts every game event by type and logs the counts every
//! minute, logs each typed game event it receives, and runs its checks
//! once the world has loaded: world key, entity queries, stats, component
//! writes, actions, spawns, a per-world store and a save on demand. Its
//! actions, for administrators on the Mods screen or from the admin tools'
//! command file, run them again and try single calls.

mod affixes;
mod buffs;
mod census;
mod checks;
mod clock;
mod damage;
mod knowledge;
mod raw_stats;
mod settings;
mod stats;
mod terrain;
mod weather;

use std::ops::ControlFlow;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use enshwick::actions::{Action, Actions, Arg, Outcome, Value};
use enshwick::ecs::Entity;
use enshwick::events::{WorldLoaded, WorldSaved};
use enshwick::game_events::{
    AltarUsed, Built, CraftRequested, Crafted, Died, EquipmentChanged, ExperienceGained, Hit,
    Interacted, Kind, LevelGained, LootOpened, PickedUp, Revived, TornDown,
};
use enshwick::i18n::Texts;
use enshwick::schedule::Scheduler;
use enshwick::{Load, Mod, Off};

use crate::census::Census;
use crate::checks::Checks;

/// How often the census is logged.
const CENSUS_PERIOD: Duration = Duration::from_secs(60);
/// How long after the world loads the checks run.
const CHECK_DELAY: Duration = Duration::from_secs(10);
/// The text `notice` shows.
const NOTICE: &str = "Probe notice";

/// Probe settings, read at start.
#[derive(Debug, Default, serde::Deserialize, serde::Serialize, enshwick::Config)]
#[serde(default, deny_unknown_fields)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent switches of a development mod's settings file"
)]
struct Settings {
    /// Exercise the platform API and log what it sees; development only.
    #[config(restart)]
    enabled: bool,
    /// Refuse every craft request, to see the craft filter in game.
    #[config(restart)]
    refuse_crafting: bool,
    /// Refuse every placement, to see the build filter in game.
    #[config(restart)]
    refuse_building: bool,
    /// Halve every damage, to see the damage filter in game.
    #[config(restart)]
    soften_damage: bool,
    /// Keep the bosses' health at 1.25 for the next start, to see a kept
    /// game setting come back with the world; the next run with this on
    /// forgets it. Off, a run keeps and forgets it at once.
    #[config(restart)]
    keep_across_restart: bool,
}

/// A check of a single feature, run after the general ones.
pub(crate) type Check = Box<dyn Fn() + Send + Sync>;

/// The checks of single features whose part of the API this build has.
fn extras(load: &mut Load<'_>, scheduler: Scheduler, settings: &Settings) -> Vec<Check> {
    let mut extras: Vec<Check> = Vec::new();
    match clock::ClockChecks::new(load, scheduler) {
        Ok(clock) => extras.push(Box::new(move || clock.run())),
        Err(off) => enshwick::warn!("FAIL clock: {off}"),
    }
    match settings::SettingsChecks::new(load, settings.keep_across_restart) {
        Ok(settings) => extras.push(Box::new(move || settings.run())),
        Err(off) => enshwick::warn!("FAIL game settings: {off}"),
    }
    match buffs::BuffChecks::new(load, scheduler) {
        Ok(buffs) => extras.push(Box::new(move || buffs.run())),
        Err(off) => enshwick::warn!("FAIL buffs: {off}"),
    }
    match knowledge::KnowledgeChecks::new(load, scheduler) {
        Ok(knowledge) => extras.push(Box::new(move || knowledge.run())),
        Err(off) => enshwick::warn!("FAIL knowledge: {off}"),
    }
    match weather::WeatherChecks::new(load, scheduler) {
        Ok(weather) => extras.push(Box::new(move || weather.run())),
        Err(off) => enshwick::warn!("FAIL weather: {off}"),
    }
    match stats::StatChecks::new(load, scheduler) {
        Ok(stats) => extras.push(Box::new(move || stats.run())),
        Err(off) => enshwick::warn!("FAIL stats: {off}"),
    }
    match terrain::TerrainChecks::new(load) {
        Ok(terrain) => extras.push(Box::new(move || terrain.run())),
        Err(off) => enshwick::warn!("FAIL terrain: {off}"),
    }
    extras
}

/// The probe.
#[derive(Debug)]
pub struct Probe;

/// Logs every event of kind `E`, or that its fields are missing.
fn log_typed<E: Kind + core::fmt::Debug>(load: &mut Load<'_>) {
    match load.on_game::<E>(|event| enshwick::info!("event: {event:?}")) {
        Ok(()) => enshwick::info!("PASS typed {} resolves", E::TYPE_NAME),
        Err(off) => enshwick::warn!("FAIL typed {}: {off}", E::TYPE_NAME),
    }
}

/// Logs the craft requests and the first placement the filters see, and
/// refuses them when the settings say so.
fn vetoes(load: &mut Load<'_>, settings: &Settings) {
    let refuse_crafting = settings.refuse_crafting;
    let crafts = load.on_craft(move |craft| {
        enshwick::info!("craft filter: {craft:?}, refused: {refuse_crafting}");
        !refuse_crafting
    });
    match crafts {
        Ok(()) => enshwick::info!("PASS craft filter registered"),
        Err(off) => enshwick::warn!("FAIL craft filter: {off}"),
    }
    let refuse_building = settings.refuse_building;
    let seen = AtomicBool::new(false);
    let builds = load.on_build(move |placement| {
        if !seen.swap(true, Ordering::Relaxed) {
            enshwick::info!("build filter, first call: {placement:?}, refused: {refuse_building}");
        }
        !refuse_building
    });
    match builds {
        Ok(()) => enshwick::info!("PASS build filter registered"),
        Err(off) => enshwick::warn!("FAIL build filter: {off}"),
    }
}

/// An id from a number argument, which the platform kept within its range.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the argument's range is within u32"
)]
const fn id(value: f64) -> u32 {
    value as u32
}

/// Declares the probe's actions, for administrators.
fn actions(
    load: &mut Load<'_>,
    census: Arc<Census>,
    checks: &Arc<Checks>,
    run_all: Arc<dyn Fn() + Send + Sync>,
    affix_checks: Option<Arc<affixes::AffixChecks>>,
) -> Result<(), Off> {
    let knowledge = load.knowledge();
    let raw_stats = raw_stats::RawStats::new(load)
        .inspect_err(|off| enshwick::warn!("FAIL raw stats: {off}"))
        .ok();
    let actions = load.actions(Texts::default())?;
    let admin = |id: &str, label: &str| Action::new(id, label).admins();
    let entity = || Arg::number("Entity", 0.0..=f64::from(u32::MAX), 1.0);
    let knowledge_id = || Arg::number("Id", 0.0..=f64::from(u32::MAX), 1.0);
    let bit = || Arg::number("Bit or amount", 0.0..=1_000_000.0, 1.0);
    let metres = |axis: &str| Arg::number(axis, -1000.0..=1000.0, 0.0);

    actions.add(admin("census", "Game events counted"), move |_| {
        Outcome::done(census.lines().join("\n"))
    })?;
    actions.add(admin("check", "Run the checks again"), move |_| {
        run_all();
        Outcome::done("checks run; see probe.log")
    })?;
    actions.add(admin("key", "Your account key"), |request| {
        Outcome::done(format!("your account key: {:016x}", request.player.key))
    })?;
    let mine = Arc::clone(checks);
    actions.add(admin("my-vitals", "Your raw stats"), move |request| {
        let me = request.player.entity;
        Outcome::done(match &raw_stats {
            Some(raw) => raw.dump(me, &[]),
            None => mine.vitals_of(me),
        })
    })?;
    let vitals = Arc::clone(checks);
    actions.add(
        admin("vitals", "An entity's vitals").arg(entity()),
        move |request| match request.args {
            [Value::Number(n)] => Outcome::done(vitals.vitals_of(Entity(id(*n)))),
            _ => Outcome::refused(""),
        },
    )?;
    let moving = Arc::clone(checks);
    actions.add(
        admin("teleport", "Move yourself by")
            .arg(metres("x"))
            .arg(metres("y"))
            .arg(metres("z")),
        move |request| match request.args {
            [Value::Number(x), Value::Number(y), Value::Number(z)] => {
                Outcome::done(moving.teleport_self(request.player.entity, [*x, *y, *z]))
            }
            _ => Outcome::refused(""),
        },
    )?;
    let destroying = Arc::clone(checks);
    actions.add(
        admin("destroy", "Destroy an entity").arg(entity()),
        move |request| match request.args {
            [Value::Number(n)] => Outcome::done(destroying.destroy(Entity(id(*n)))),
            _ => Outcome::refused(""),
        },
    )?;
    let spawning = Arc::clone(checks);
    actions.add(
        admin("spawn", "Spawn a copy of an entity").arg(entity()),
        move |request| match request.args {
            [Value::Number(n)] => Outcome::done(spawning.spawn_copy(Entity(id(*n)))),
            _ => Outcome::refused(""),
        },
    )?;
    let noticing = Arc::clone(checks);
    actions.add(admin("notice", "Show yourself a notice"), move |request| {
        Outcome::done(noticing.notice(request.player, NOTICE))
    })?;
    for (kind, label, with_bit) in [
        ("knowledge", "Read a knowledge value", false),
        ("learn", "Learn a knowledge value", true),
        ("unlearn", "Unlearn a knowledge value", true),
    ] {
        let action = admin(kind, label).arg(knowledge_id());
        let action = if with_bit { action.arg(bit()) } else { action };
        actions.add(action, move |request| {
            let (known, bit) = match request.args {
                [Value::Number(known)] => (id(*known), None),
                [Value::Number(known), Value::Number(bit)] => (id(*known), Some(id(*bit))),
                _ => return Outcome::refused(""),
            };
            let me = request.player.entity;
            Outcome::done(knowledge::command(knowledge, me, kind, known, bit))
        })?;
    }
    match affix_checks {
        Some(affix_checks) => stat_actions(&actions, &affix_checks),
        None => Ok(()),
    }
}

/// Declares the probe's actions on stats and affixes.
fn stat_actions(actions: &Actions, affix_checks: &Arc<affixes::AffixChecks>) -> Result<(), Off> {
    let admin = |id: &str, label: &str| Action::new(id, label).admins();
    let ids: Vec<String> = affix_checks.kind_ids();
    let kinds: Vec<(&str, &str)> = ids.iter().map(|id| (id.as_str(), id.as_str())).collect();
    let (setting, ids_set) = (Arc::clone(affix_checks), ids.clone());
    actions.add(
        admin("stat", "Add to one of your stats")
            .arg(Arg::choice("Stat", &kinds))
            .arg(Arg::number("Value", -1000.0..=1000.0, 0.0)),
        move |request| match request.args {
            [Value::Choice(kind), Value::Number(value)] => {
                Outcome::done(setting.stat(request.player, &ids_set[*kind], &value.to_string()))
            }
            _ => Outcome::refused(""),
        },
    )?;
    let (removing, ids_off) = (Arc::clone(affix_checks), ids.clone());
    actions.add(
        admin("stat-off", "Remove the probe's change to a stat").arg(Arg::choice("Stat", &kinds)),
        move |request| match request.args {
            [Value::Choice(kind)] => {
                Outcome::done(removing.stat(request.player, &ids_off[*kind], "off"))
            }
            _ => Outcome::refused(""),
        },
    )?;
    let reading = Arc::clone(affix_checks);
    let rolling = Arc::clone(affix_checks);
    actions.add(admin("stats", "Your changed stats"), move |request| {
        Outcome::done(reading.stats_of(request.player))
    })?;
    actions.add(admin("affix", "Roll affixes again"), move |request| {
        Outcome::done(rolling.reroll(request.player))
    })
}

impl Mod for Probe {
    fn manifest() -> &'static enshwick::abi::Manifest {
        enshwick::manifest!(id: "probe", side: SERVER, name: "Probe")
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
            Err(off) => {
                enshwick::info!("PASS a channel is refused to a mod that is no join mod: {off}");
            }
            Ok(_) => {
                enshwick::warn!("FAIL a channel opened for a mod that is no join mod");
            }
        }
        let checks = Arc::new(Checks::new(load)?);
        let ping = load.event_type("keen::ecs::UiPingEvent")?;
        let pinged = Arc::clone(&checks);
        load.on_raw_game_event(ping, move |event| pinged.pinged(event))?;
        let scheduler = load.scheduler()?;
        let damage = match damage::DamageSeen::register(load, settings.soften_damage) {
            Ok(seen) => {
                enshwick::info!("PASS damage filter registered");
                Some(seen)
            }
            Err(off) => {
                enshwick::warn!("FAIL damage filter: {off}");
                None
            }
        };
        let logged = Arc::clone(&census);
        scheduler.every(CENSUS_PERIOD, move || {
            for line in logged.lines() {
                enshwick::info!("census: {line}");
            }
            if let Some(damage) = &damage {
                enshwick::info!("{}", damage.line());
            }
            ControlFlow::Continue(())
        });
        let mut extras = extras(load, scheduler, &settings);
        let affix_checks = affixes::AffixChecks::register(load, &mut extras);
        let extras: Arc<[Check]> = extras.into();
        let run_all = {
            let checks = Arc::clone(&checks);
            move || {
                checks.run();
                for extra in extras.iter() {
                    extra();
                }
            }
        };
        let run_all: Arc<dyn Fn() + Send + Sync> = Arc::new(run_all);
        let first = Arc::clone(&run_all);
        load.on::<WorldLoaded>(move |()| {
            let run = Arc::clone(&first);
            scheduler.after(CHECK_DELAY, move || run());
        })?;
        let saved = Arc::clone(&checks);
        load.on::<WorldSaved>(move |()| saved.saved())?;
        actions(load, census, &checks, run_all, affix_checks)?;
        enshwick::info!("probe on");
        Ok(Self)
    }
}

enshwick::declare_mod!(Probe);
