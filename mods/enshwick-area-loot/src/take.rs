//! The server's side: a link after the game's loot container check notices a
//! container a player emptied and queues the full ones around it; each is
//! then emptied into that player, nearest first, through the game's own
//! inventory transactions.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use enshwick_loader_api::ecs::{Entity, Worlds};
use enshwick_loader_api::inventory::{Batch, Inventories, Op, Outcome};
use enshwick_loader_api::loot::{Container, Containers, near};
use enshwick_loader_api::{Load, Next, Off};

use crate::Settings;
use crate::tracker::Tracker;

/// Another player's interaction keeps a container out for this long.
const BUSY: Duration = Duration::from_secs(10);
/// Another player who last opened a container and stands this close still
/// has it open.
const OPEN_RANGE: f64 = 4.0;
/// How often the pass prunes what it remembers.
const PRUNE_EVERY: Duration = Duration::from_secs(60);

/// The reads the pass needs.
#[derive(Debug)]
struct Engine {
    containers: Containers,
    inventories: Inventories,
    worlds: Worlds,
}

static ENGINE: OnceLock<Engine> = OnceLock::new();
static TRACKER: Mutex<Option<Tracker>> = Mutex::new(None);
static LAST_PRUNE: Mutex<Option<Instant>> = Mutex::new(None);
static ENABLED: AtomicBool = AtomicBool::new(true);
/// Radius and height in metres, as `f32` bits.
static RADIUS: AtomicU32 = AtomicU32::new(0x4140_0000);
static HEIGHT: AtomicU32 = AtomicU32::new(0x4080_0000);
static OFF: AtomicBool = AtomicBool::new(false);

enshwick_loader_api::detour! {
    static CHECK: unsafe extern "system" fn(ctx: *mut u64) = check;
}

/// Applies `settings` from now on.
pub(crate) fn apply(settings: &Settings) {
    ENABLED.store(settings.enabled, Ordering::Relaxed);
    RADIUS.store(settings.radius.to_bits(), Ordering::Relaxed);
    HEIGHT.store(settings.height.to_bits(), Ordering::Relaxed);
}

fn tracker() -> MutexGuard<'static, Option<Tracker>> {
    TRACKER.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Links the pass.
pub(crate) fn link(load: &mut Load<'_>) -> Result<(), Off> {
    let engine = Engine {
        containers: Containers::resolve(load)?,
        inventories: load.inventories()?,
        worlds: Worlds::resolve(load)?,
    };
    let _first = ENGINE.set(engine);
    *tracker() = Some(Tracker::default());
    load.hook(&CHECK, load.system(enshwick_loader_api::loot::SYSTEM)?)?;
    // Entity ids belong to one game.
    load.on::<enshwick_loader_api::events::Shutdown>(|()| {
        if let Some(t) = tracker().as_mut() {
            t.clear();
        }
    })
}

fn check(next: Next<unsafe extern "system" fn(*mut u64)>, ctx: *mut u64) {
    // SAFETY: the original system takes its own context.
    unsafe { next.get()(ctx) };
    if OFF.load(Ordering::Acquire) || !ENABLED.load(Ordering::Relaxed) {
        return;
    }
    if let Some(engine) = ENGINE.get()
        // SAFETY: the system's live context.
        && unsafe { engine.worlds.is_server(ctx) }
        // SAFETY: the system's live context, after vanilla ran.
        && std::panic::catch_unwind(|| unsafe { pass(engine, ctx) }).is_err()
        && !OFF.swap(true, Ordering::AcqRel)
    {
        enshwick_loader_api::warn!("area loot off, vanilla from now on: the pass panicked");
    }
}

/// Watches every container and starts a take for each one a player emptied.
///
/// # Safety
///
/// `ctx` is the live context of the loot container check, on its thread.
unsafe fn pass(engine: &Engine, ctx: *mut u64) {
    let now = Instant::now();
    let mut emptied: Vec<(Container, Entity)> = Vec::new();
    {
        let mut guard = tracker();
        let Some(t) = guard.as_mut() else {
            return;
        };
        // SAFETY: forwarded from the caller.
        unsafe {
            engine.containers.each(ctx, |c| {
                if let Some(by) = t.observe(c, now) {
                    emptied.push((*c, by));
                }
            });
        }
        let mut last = LAST_PRUNE.lock().unwrap_or_else(PoisonError::into_inner);
        if last.is_none_or(|at| now.saturating_duration_since(at) >= PRUNE_EVERY) {
            t.prune(now);
            *last = Some(now);
        }
    }
    for (container, player) in emptied {
        // SAFETY: forwarded from the caller.
        if container.is_enemy_loot() && unsafe { engine.containers.is_player(ctx, player) } {
            // SAFETY: as above.
            let queue = unsafe { gather(engine, ctx, &container, player, now) };
            if !queue.is_empty() {
                enshwick_loader_api::debug!(
                    "{} emptied {}: taking {} more",
                    player.0,
                    container.entity.0,
                    queue.len()
                );
                take_next(player, queue);
            }
        }
    }
}

/// The full enemy loot containers around `emptied` that no other player is
/// at, nearest first, each held for the take.
///
/// # Safety
///
/// As for [`pass`].
unsafe fn gather(
    engine: &Engine,
    ctx: *mut u64,
    emptied: &Container,
    player: Entity,
    now: Instant,
) -> VecDeque<Entity> {
    let radius = f64::from(f32::from_bits(RADIUS.load(Ordering::Relaxed)));
    let height = f64::from(f32::from_bits(HEIGHT.load(Ordering::Relaxed)));
    let mut guard = tracker();
    let Some(t) = guard.as_mut() else {
        return VecDeque::new();
    };
    let mut queue = VecDeque::new();
    for entity in t.around(emptied.position, radius, height, emptied.entity, now) {
        // SAFETY: forwarded from the caller.
        let Some(c) = (unsafe { engine.containers.container(ctx, entity) }) else {
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
            // SAFETY: as above.
            let at = unsafe { engine.containers.position(ctx, guest) };
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
fn take_next(player: Entity, mut queue: VecDeque<Entity>) {
    let Some(engine) = ENGINE.get() else {
        return;
    };
    let Some(container) = queue.pop_front() else {
        return;
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
    let inventories = engine.inventories;
    let _ticket = inventories.submit(&batch, move |outcome: &Outcome| {
        let left = inventories
            .read()
            .slots(container)
            .iter()
            .any(|s| !s.is_empty());
        if emptied(outcome) && !left {
            take_next(player, queue);
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
