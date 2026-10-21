# Pacing and threads

The engine paces its loops with waits on handles and sizes its thread pools
from the CPUs it sees. [`pacing`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/pacing.rs)
names the places a mod can change to make the game use less CPU. Each
function hooks its site during `init`, finds it by its bytes on the running
build and tells whether it was found.

| Function | Site | Where |
| --- | --- | --- |
| `on_loop_wait` | the event loop's wait on its handles, with the milliseconds it is about to wait and the time left before its deadline | server and client; the client's message wait too |
| `on_server_wait` | the server loop's wait between two passes, in nanoseconds | the game server |
| `cap_task_workers` | the most worker threads each task group starts with | the dedicated server only |

## Short waits

The event loop turns the time left before its deadline into whole
milliseconds, truncating. With less than a millisecond left it waits 0 ms,
again and again, until the deadline passes. A handler that sleeps that time
instead saves the polling; it must never sleep past `left_ns`, and it leaves
the wait itself to the game, which then sees every event:

```rust
use enshwick_loader_api::pacing;

fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    let found = pacing::on_loop_wait(load, |wait| {
        if wait.ms == 0 && wait.left_ns > 0 {
            // Sleep a little less than `left_ns` here.
        }
    })?;
    if !found.handles {
        enshwick_loader_api::warn!("the event loop's wait was not found");
    }
    Ok(())
}
```

The handler runs on the waiting thread, as often as the loop waits: it is
[code in the game's frame](game-frame.md).

## The server loop

The server loop waits [`SERVER_IDLE_WAIT_NS`] with nobody online,
[`SERVER_ONLINE_WAIT_NS`] with players, and [`SERVER_SLOW_WAIT_NS`] after a
pass longer than 14 ms. A handler may change the wait; an event still ends it
early. A longer wait with players online means fewer passes between the
simulation steps, and up to that much more latency for what arrives between
them.

```rust
use enshwick_loader_api::pacing;

fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    pacing::on_server_wait(load, |wait_ns| {
        if *wait_ns == pacing::SERVER_ONLINE_WAIT_NS {
            *wait_ns = 2 * pacing::SERVER_ONLINE_WAIT_NS;
        }
    })?;
    Ok(())
}
```

## Task workers

The engine starts a worker thread per logical CPU in each task group, up to
its own limits. Under Wine those are the host's CPUs, whatever a container
allows. `cap_task_workers` lowers the limits as the task system starts. Only
the dedicated server starts it after the loader arms its hooks (see
[the life of a mod](concepts/lifecycle.md)); on the client it returns
`false`. The game's log shows the result, `[task] Group 'Default'
Worker:…`.

```rust
use enshwick_loader_api::pacing::{self, TaskWorkers};

fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    if !pacing::cap_task_workers(load, TaskWorkers::at_most(4))? {
        enshwick_loader_api::info!("the task workers stay as the game sizes them");
    }
    Ok(())
}
```

[`SERVER_IDLE_WAIT_NS`]: https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/pacing.rs
[`SERVER_ONLINE_WAIT_NS`]: https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/pacing.rs
[`SERVER_SLOW_WAIT_NS`]: https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/pacing.rs
