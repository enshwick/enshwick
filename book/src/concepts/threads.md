# Threads

A mod's code runs on threads it does not own. Which thread decides what the
code may call.

## The threads

- **The game thread**: the server's main loop. On the dedicated server it is
  the server's game thread; on a client that hosts a game it is the built-in
  server's `GameServer` thread, a new one for each hosted game. Game events,
  scheduled tasks and chat command handlers run here, between simulation
  steps.
- **ECS worker threads**: the engine runs systems in parallel on them. A hook
  on a system's update function runs on whichever worker runs the system.
  Many other hooked game functions are called from systems too.
- **Loader threads**: the loader's own. `on_config_change` handlers run on the
  settings watcher, about a second after a file changes; on a client joining a
  server, join values reach the handler on the thread that runs the join.
  Status lines from `load.status` are gathered on the status writer or on the
  game thread.
- **The loader's start thread** runs `init` and `ready`.
- **The mod's own threads**, started in `ready`, for slow work: files, network,
  anything that waits.

## What may be called where

| Call | Where |
| --- | --- |
| world reads (`position`, `component`, `players`, ...), `set_label` | the game thread; elsewhere they return `None` or nothing |
| `commands::run` | the game thread |
| `inventories.read()` | the game thread, between steps |
| `inventories.read_in(ctx)`, `ecs` reads | inside a hooked system, through its context |
| `scheduler.next_tick`, `after`, `every` | any thread; the task runs on the game thread |
| `inventories.submit`, `give`, `take`, moves | any thread; the callback runs on the game thread |
| `world.broadcast`, `world.tell` | any thread; posted after the next step |
| `host()`, logging, `host.fault` | any thread |

The usual shape: a hook on a worker thread notes what happened in an atomic or
a queue, and a scheduled task or a game event acts on it on the game thread.

```rust
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

static PENDING: AtomicU32 = AtomicU32::new(0);

fn init(load: &mut enshwick_api::Load<'_>) -> Result<(), enshwick_api::Off> {
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

Hooks run often and on hot threads: keep them free of locks, allocation and
logging in the common case. [Code in the game's frame](../game-frame.md) has
the rules.
