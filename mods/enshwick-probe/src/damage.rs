//! Damage filters: counts the damage the filter sees by cause, logs each
//! damage but hits and the hits on or by a player, and halves every damage
//! when the settings say so.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use enshwick::damage::{Damage, DamageCause};
use enshwick::ecs::Entity;
use enshwick::world::World;
use enshwick::{Load, Off};

/// The most hits on or by a player logged one by one.
const MOST_HIT_LINES: u32 = 200;

/// The causes counted, in the order of [`DamageSeen::counts`].
const CAUSES: [DamageCause; 8] = [
    DamageCause::Hit,
    DamageCause::Fall,
    DamageCause::Shroud,
    DamageCause::ShroudDeath,
    DamageCause::Drowning,
    DamageCause::DrowningDeath,
    DamageCause::Freezing,
    DamageCause::AdminKill,
];

/// What the damage filter saw, by cause.
#[derive(Debug, Default)]
pub(crate) struct DamageSeen {
    counts: [AtomicU64; CAUSES.len()],
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
            let cause = damage.cause();
            if let Some(index) = CAUSES.iter().position(|&c| c == cause) {
                counted.counts[index].fetch_add(1, Ordering::Relaxed);
            }
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
        enshwick::info!(
            "R10 damage {cause:?} to {}, from {}, {before} before the filter, {} after",
            who(target),
            source.map_or_else(|| "nothing".to_owned(), who),
            damage.total()
        );
    }

    /// The counts, for the log.
    pub(crate) fn line(&self) -> String {
        let counts: Vec<String> = CAUSES
            .iter()
            .zip(&self.counts)
            .map(|(cause, count)| format!("{cause:?} {}", count.load(Ordering::Relaxed)))
            .collect();
        format!("damage seen: {}", counts.join(", "))
    }
}
