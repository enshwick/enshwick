//! Stats: a kind of the probe's own and built-in ones opened, modifiers set
//! on an entity with health, its values and maximum health read back after
//! a step, and the modifiers removed again.

use std::time::Duration;

use enshwick::ecs::Entity;
use enshwick::schedule::Scheduler;
use enshwick::stats::{Lasting, Modifier, Op, Stat, StatDef, Stats, builtin};
use enshwick::world::{ComponentType, World};
use enshwick::{Load, Off};

/// How long the probe waits for the values to follow.
const SETTLE: Duration = Duration::from_secs(1);
/// The extra maximum health the probe gives.
const EXTRA: i16 = 100;
/// The probe's key for its modifiers.
const KEY: &str = "probe";

/// The stats checks.
#[derive(Debug, Clone, Copy)]
pub(crate) struct StatChecks {
    stats: Stats,
    world: World,
    health: ComponentType,
    scheduler: Scheduler,
    luck: Stat,
    health_max: Stat,
    fire: Stat,
}

impl StatChecks {
    pub(crate) fn new(load: &mut Load<'_>, scheduler: Scheduler) -> Result<Self, Off> {
        let world = load.world()?;
        let luck = load.define_stat("luck", &StatDef::additive("probe.luck").range(0.0, 1.0))?;
        load.on_stats_changed(|change| {
            enshwick::info!(
                "stats: {:?} stat {} {} -> {}",
                change.entity,
                change.stat.index(),
                change.old,
                change.new
            );
        })?;
        Ok(Self {
            stats: load.stats()?,
            health: world.component_type(load, "keen::ecs::Health")?,
            world,
            scheduler,
            luck,
            health_max: load.open_stat(builtin::HEALTH_MAX)?,
            fire: load.open_stat(builtin::RESISTANCE_FIRE)?,
        })
    }

    pub(crate) fn run(self) {
        match self.stats.info(self.fire) {
            Some(info) if info.id == builtin::RESISTANCE_FIRE => {
                enshwick::info!("PASS stats: built-in kind {info:?}");
            }
            other => enshwick::warn!("FAIL stats: built-in kind reads {other:?}"),
        }
        let Some(target) = self
            .world
            .entities(self.health)
            .iter()
            .map(|p| p.entity)
            .find(|&e| self.world.vitals(e).is_some_and(|v| v.health.current > 1))
        else {
            enshwick::info!("SKIP stats: no living entity with health");
            return;
        };
        let before = self.world.vitals(target).map(|v| v.health.max);
        let set = [
            Modifier::new(self.luck, Op::Add, 0.25),
            Modifier::new(self.health_max, Op::Add, f32::from(EXTRA)),
        ]
        .into_iter()
        .try_for_each(|m| self.stats.set(target, KEY, m, Lasting::UntilRemoved));
        if let Err(why) = set {
            enshwick::warn!("FAIL stats: modifiers on {target:?}: {why}");
            return;
        }
        self.scheduler
            .after(SETTLE, move || self.raised(target, before));
    }

    fn raised(self, target: Entity, before: Option<i32>) {
        let luck = self.stats.value(target, self.luck);
        if (luck - 0.25).abs() < 1e-5 {
            enshwick::info!("PASS stats: luck of {target:?} reads {luck}");
        } else {
            enshwick::warn!("FAIL stats: luck of {target:?} reads {luck}");
        }
        let sources = self.stats.sources(target, None);
        enshwick::info!("stats: sources of {target:?}: {sources:?}");
        let after = self.world.vitals(target).map(|v| v.health.max);
        match (before, after) {
            (Some(b), Some(a)) if a == b + i32::from(EXTRA) => {
                enshwick::info!("PASS stats: maximum health of {target:?} {b} -> {a}");
            }
            other => {
                enshwick::warn!("FAIL stats: maximum health of {target:?}: {other:?}");
            }
        }
        let removed = [self.luck, self.health_max]
            .into_iter()
            .try_for_each(|stat| self.stats.remove(target, KEY, stat));
        if let Err(why) = removed {
            enshwick::warn!("FAIL stats: removal on {target:?}: {why}");
            return;
        }
        self.scheduler.after(SETTLE, move || {
            let back = self.world.vitals(target).map(|v| v.health.max);
            if back == before {
                enshwick::info!("PASS stats: maximum health of {target:?} back to {back:?}");
            } else {
                enshwick::warn!(
                    "FAIL stats: maximum health of {target:?} {back:?}, was {before:?}"
                );
            }
        });
    }
}
