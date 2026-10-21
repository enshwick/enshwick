//! After each loot container check the mod notices a container a player
//! emptied and queues the full ones around it; each is then emptied into that
//! player, nearest first, through the game's own inventory transactions,
//! once it is checked again that nobody else opened it meanwhile.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use crate::clock::Clock;

use enshwick_loader_api::ecs::Entity;
use enshwick_loader_api::inventory::{Batch, Inventories, Op, Outcome};
use enshwick_loader_api::loot::{Container, LootPass, near};
use enshwick_loader_api::{Load, Off};

use crate::Settings;
use crate::tracker::Tracker;

/// Another player's interaction keeps a container out for this long.
const BUSY: Duration = Duration::from_secs(10);
/// Another player who last opened a container and stands this close still
/// has it open.
const OPEN_RANGE: f64 = 4.0;
/// How often the pass prunes what it remembers.
const PRUNE_EVERY: Duration = Duration::from_secs(60);

static INVENTORIES: OnceLock<Inventories> = OnceLock::new();
static TRACKER: Mutex<Option<Tracker>> = Mutex::new(None);
static LAST_PRUNE: Mutex<Option<Duration>> = Mutex::new(None);
/// The time the holds and the busy window count in: the server's steps.
static CLOCK: Clock = Clock::new();
static ENABLED: AtomicBool = AtomicBool::new(true);
/// Radius and height in metres, as `f32` bits.
static RADIUS: AtomicU32 = AtomicU32::new(0x4140_0000);
static HEIGHT: AtomicU32 = AtomicU32::new(0x4080_0000);
static OFF: AtomicBool = AtomicBool::new(false);
/// Counts the games run, so a take chain never reaches into the next one.
static GAME: AtomicU64 = AtomicU64::new(0);

/// Applies `settings` from now on.
pub(crate) fn apply(settings: &Settings) {
    let was = ENABLED.swap(settings.enabled, Ordering::Relaxed);
    // Off, the tracker stood still: what was emptied meanwhile must not count
    // as emptied now, with its loot sent to whoever opened it last.
    if settings.enabled
        && !was
        && let Some(t) = tracker().as_mut()
    {
        t.clear();
    }
    RADIUS.store(settings.radius.to_bits(), Ordering::Relaxed);
    HEIGHT.store(settings.height.to_bits(), Ordering::Relaxed);
}

fn tracker() -> MutexGuard<'static, Option<Tracker>> {
    TRACKER.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Links the pass.
pub(crate) fn link(load: &mut Load<'_>) -> Result<(), Off> {
    let _first = INVENTORIES.set(load.inventories()?);
    *tracker() = Some(Tracker::default());
    load.on::<enshwick_loader_api::events::Ticked>(|_| CLOCK.step(Instant::now()))?;
    load.on_loot_pass(check)?;
    // Entity ids belong to one game.
    load.on::<enshwick_loader_api::events::ShutdownStarted>(|()| {
        GAME.fetch_add(1, Ordering::AcqRel);
        if let Some(t) = tracker().as_mut() {
            t.clear();
        }
    })
}

fn check(loot: &LootPass<'_>) {
    if OFF.load(Ordering::Acquire) || !ENABLED.load(Ordering::Relaxed) {
        return;
    }
    if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| pass(loot))).is_err()
        && !OFF.swap(true, Ordering::AcqRel)
    {
        enshwick_loader_api::warn!("area loot off, vanilla from now on: the pass panicked");
    }
}

/// Watches every container and starts a take for each one a player emptied.
fn pass(loot: &LootPass<'_>) {
    let now = CLOCK.now();
    let mut emptied: Vec<(Container, Entity)> = Vec::new();
    {
        let mut guard = tracker();
        let Some(t) = guard.as_mut() else {
            return;
        };
        loot.each(|c| {
            if let Some(by) = t.observe(c, now) {
                emptied.push((*c, by));
            }
        });
        let mut last = LAST_PRUNE.lock().unwrap_or_else(PoisonError::into_inner);
        if last.is_none_or(|at| now.saturating_sub(at) >= PRUNE_EVERY) {
            t.prune(now);
            *last = Some(now);
        }
    }
    for (container, player) in emptied {
        if container.is_enemy_loot() && loot.is_player(player) {
            let queue = gather(loot, &container, player, now);
            if !queue.is_empty() {
                enshwick_loader_api::debug!(
                    "{} emptied {}: taking {} more",
                    player.0,
                    container.entity.0,
                    queue.len()
                );
                take_next(player, queue, GAME.load(Ordering::Acquire));
            }
        }
    }
}

/// The full enemy loot containers around `emptied` that no other player is
/// at, nearest first, each held for the take.
fn gather(
    loot: &LootPass<'_>,
    emptied: &Container,
    player: Entity,
    now: Duration,
) -> VecDeque<Entity> {
    let radius = f64::from(f32::from_bits(RADIUS.load(Ordering::Relaxed)));
    let height = f64::from(f32::from_bits(HEIGHT.load(Ordering::Relaxed)));
    let mut guard = tracker();
    let Some(t) = guard.as_mut() else {
        return VecDeque::new();
    };
    let mut queue = VecDeque::new();
    for entity in t.around(emptied.position, radius, height, emptied.entity, now) {
        let Some(c) = loot.container(entity) else {
            t.forget(entity);
            continue;
        };
        if !c.is_open() || !near(emptied.position, c.position, radius, height) {
            continue;
        }
        if t.busy(&c, player, now, BUSY) {
            continue;
        }
        let guest = c.offer.map_or(Entity::NONE, |o| o.guest);
        if guest != player && !guest.is_none() {
            let at = loot.position(guest);
            if at.is_some_and(|at| near(c.position, at, OPEN_RANGE, OPEN_RANGE)) {
                continue;
            }
        }
        t.hold(entity, now);
        queue.push_back(entity);
    }
    queue
}

/// Empties the first container of `queue` into `player`, and the next once
/// that one emptied completely.
fn take_next(player: Entity, mut queue: VecDeque<Entity>, game: u64) {
    let Some(&inventories) = INVENTORIES.get() else {
        return;
    };
    // Switched off, stopped by a panic or another game since: the rest stays.
    if OFF.load(Ordering::Acquire)
        || !ENABLED.load(Ordering::Relaxed)
        || GAME.load(Ordering::Acquire) != game
    {
        if let Some(t) = tracker().as_mut() {
            for rest in queue {
                t.release(rest);
            }
        }
        return;
    }
    // Each container is checked again right before its turn: another
    // player may have opened it since the take gathered it.
    let container = loop {
        let Some(next) = queue.pop_front() else {
            return;
        };
        let mut guard = tracker();
        let Some(t) = guard.as_mut() else {
            return;
        };
        if t.still_free(next, player, CLOCK.now()) {
            break next;
        }
        t.release(next);
        enshwick_loader_api::debug!("area loot for {} skips {}: opened since", player.0, next.0);
    };
    let batch = Batch::new()
        .with(Op::MoveAll {
            from: container,
            to: player,
            include_special: false,
        })
        .actor(player)
        .allow_partial()
        .notify();
    let _ticket = inventories.submit(&batch, move |outcome: &Outcome| {
        let left = inventories
            .read()
            .slots(container)
            .iter()
            .any(|s| !s.is_empty());
        if emptied(outcome) && !left {
            take_next(player, queue, game);
            return;
        }
        // Inventory full or the container changed: the rest stays for later.
        enshwick_loader_api::debug!(
            "area loot for {} stopped at {}: {:?}",
            player.0,
            container.0,
            outcome.done
        );
        if let Some(t) = tracker().as_mut() {
            t.release(container);
            for rest in queue {
                t.release(rest);
            }
        }
    });
}

/// Whether the take committed with every operation run; what is left in
/// the container is checked apart.
fn emptied(outcome: &Outcome) -> bool {
    outcome.is_committed() && outcome.ops.iter().all(|r| r.ok())
}

#[cfg(test)]
mod tests {
    use enshwick_loader_api::inventory::{Done, OpResult};

    use super::*;

    #[test]
    fn only_a_full_commit_empties() {
        let full = |error, remaining| Outcome {
            done: Done::Committed,
            ops: vec![OpResult { error, remaining }],
        };
        assert!(emptied(&full(0, 0)));
        assert!(!emptied(&full(OpResult::NO_ROOM, 0)));
        assert!(!emptied(&Outcome {
            done: Done::RolledBack,
            ops: vec![OpResult {
                error: 0,
                remaining: 0
            }],
        }));
    }

    #[test]
    fn settings_reach_the_pass() {
        apply(&Settings {
            radius: 20.0,
            height: 2.5,
            enabled: false,
        });
        assert!(!ENABLED.load(Ordering::Relaxed));
        assert!((f32::from_bits(RADIUS.load(Ordering::Relaxed)) - 20.0).abs() < f32::EPSILON);
        assert!((f32::from_bits(HEIGHT.load(Ordering::Relaxed)) - 2.5).abs() < f32::EPSILON);
        apply(&Settings::default());
        assert!(ENABLED.load(Ordering::Relaxed));
    }
}
