<!--
type: explanation
description: A mod's code runs on threads it does not own: the game thread, ECS worker threads and the loader's threads. The thread decides what the code may call.
-->

# Threads

A mod's code runs on threads it does not own. Which thread decides what the
code may call.

## The threads

- **The game thread**: the server's main loop. On the dedicated server it is
  the server's game thread. On a client that hosts a game it is the built-in
  server's `GameServer` thread, a new one for each hosted game. Game events,
  scheduled tasks and action handlers run here, between simulation
  steps. The game thread also runs ECS systems while it steps. So a filter
  or a hook may run on it in the middle of a step.
- **ECS worker threads**: the engine runs systems in parallel on them. A hook
  on a system's update function runs on whichever worker runs the system.
  Many other hooked game functions are called from systems too.
- **Loader threads**: the loader's own. Settings handlers run on the game
  thread as a world loads or stops. `on_settings_change` handlers also run on
  other threads, so a handler cannot count on one thread:
  - the settings watcher, about a second after a file changes;
  - the thread that made a change from the console or the Mods screen;
  - on a client, the thread that took the server's values.

  Status lines from `load.add_status` are gathered on the status writer or on the
  game thread.
- **The loader's start thread** runs `init` and `ready`.
- **The mod's own threads**, started in `ready`, for slow work: files, network,
  anything that waits.

## What may be called where

| Call | Where |
| --- | --- |
| world reads | the game thread; elsewhere they return `None` or nothing |
| world changes and the reads of those features | the game thread, between steps; inside a step, such as in a filter or a hook on a system, they return `WrongThread` |
| `console::run` | the game thread |
| `inventories.read()` | the game thread, between steps |
| `inventories.read_in(ctx)`, `ecs` reads | inside a hooked system, through its context |
| `scheduler.next_tick` | any thread; the task runs on the game thread |
| the scheduler's `after` and `every` | any thread; the task runs on the game thread |
| `inventories.submit` | any thread; the callback runs on the game thread |
| the inventories' `give`, `take` and moves | any thread; the callback runs on the game thread |
| `world.broadcast`, `world.tell` | any thread; posted after the next step |
| `host()`, logging, `host.fault` | any thread |

World reads include these calls:

- `position` and `component`
- `players`

World changes include these calls and features:

- `write` and `set_label`
- `spawn` and `teleport`
- the clock, weather, buffs, knowledge and terrain
- `console::run`

The usual shape has two parts. A hook on a worker thread notes what happened
in an atomic or a queue. Then a scheduled task or a game event acts on it on
the game thread.

```rust
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

static PENDING: AtomicU32 = AtomicU32::new(0);

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let scheduler = load.scheduler()?;
    let world = load.world()?;
    scheduler.every(Duration::from_secs(1), move || {
        let seen = PENDING.swap(0, Ordering::Relaxed);
        if seen > 0 {
            world.broadcast(&format!("{seen} things happened"));
        }
        std::ops::ControlFlow::Continue(())
    });
    Ok(())
}
```

A panic turns off only the feature it hit. So a lock that a panicking thread
held stays poisoned while the rest of the mod runs. Take a `std` lock through
one of these traits:

- [`Relock`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/locks.rs), with
  `relock()`
- [`RelockRw`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/locks.rs),
  with `relock_read()` and more
- [`RelockWait`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/locks.rs),
  with `relock_wait(guard)`

They keep the state the panicking thread left and clear the poison. They also
log the line that took the lock.

Hooks run often and on hot threads: keep them free of locks, allocation and
logging in the common case. [Code in the game's frame](../game-frame.md) has
the rules.
