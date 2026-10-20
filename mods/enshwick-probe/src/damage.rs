//! Damage filters: counts the damage the filter sees by cause, logs each
//! fall and shroud damage and the hits on or by a player, and halves every
//! damage when the settings say so.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use enshwick_loader_api::damage::{Damage, DamageCause};
use enshwick_loader_api::ecs::Entity;
use enshwick_loader_api::world::World;
use enshwick_loader_api::{Load, Off};

/// The most hits on or by a player logged one by one.
const MOST_HIT_LINES: u32 = 200;

/// What the damage filter saw, by cause: hits, falls, shroud, deadly shroud.
#[derive(Debug, Default)]
pub(crate) struct DamageSeen {
    counts: [AtomicU64; 4],
    hit_lines: AtomicU32,
}

impl DamageSeen {
    /// Registers the probe's damage filter; it halves every damage when
    /// `soften` is on.
    pub(crate) fn register(load: &mut Load<'_>, soften: bool) -> Result<Arc<Self>, Off> {
        let world = load.world()?;
        let seen = Arc::new(Self::default());
        let counted = Arc::clone(&seen);
        load.on_damage(move |damage| {
            let index = match damage.cause() {
                DamageCause::Hit => 0,
                DamageCause::Fall => 1,
                DamageCause::Shroud => 2,
                _ => 3,
            };
            counted.counts[index].fetch_add(1, Ordering::Relaxed);
            let before = damage.total();
            if soften {
                damage.scale(0.5);
            }
            counted.note(world, damage, before);
        })?;
        Ok(seen)
    }

    /// Logs one damage: every one but a hit, and a hit when a player takes
    /// or deals it, up to [`MOST_HIT_LINES`].
    fn note(&self, world: World, damage: &Damage<'_>, before: f32) {
        let who = |entity: Entity| match world.player(entity) {
            Some(player) => format!("entity {} (player {})", entity.0, player.name),
            None => format!("entity {}", entity.0),
        };
        let target = damage.target();
        let source = damage.source();
        let cause = damage.cause();
        if cause == DamageCause::Hit {
            let by_player = source.is_some_and(|s| world.player(s).is_some());
            if world.player(target).is_none() && !by_player {
                return;
            }
            if self.hit_lines.fetch_add(1, Ordering::Relaxed) >= MOST_HIT_LINES {
                return;
            }
        }
        enshwick_loader_api::info!(
            "R10 damage {cause:?} to {}, from {}, {before} before the filter, {} after",
            who(target),
            source.map_or_else(|| "nothing".to_owned(), who),
            damage.total()
        );
    }

    /// The counts, for the log.
    pub(crate) fn line(&self) -> String {
        let [hits, falls, shroud, death] = &self.counts;
        format!(
            "damage seen: {} hits, {} falls, {} shroud, {} deadly shroud",
            hits.load(Ordering::Relaxed),
            falls.load(Ordering::Relaxed),
            shroud.load(Ordering::Relaxed),
            death.load(Ordering::Relaxed),
        )
    }
}
