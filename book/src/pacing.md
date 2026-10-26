# Pacing and threads

The engine paces its loops with waits on handles and sizes its thread pools
from the CPUs it sees. [`pacing`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/pacing.rs)
names the places a mod can change to make the game use less CPU. Each
function asks the loader during `init`; the loader finds the place in the
running build's table, hooks it and tells whether the build has it. Handlers
of several mods run in load order, and with worker caps the lowest wins.

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
fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let found = load.on_loop_wait(|wait| {
        if wait.ms == 0 && wait.left_ns > 0 {
            // Sleep a little less than `left_ns` here.
        }
    })?;
    if !found.handles {
        enshwick::warn!("the event loop's wait was not found");
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
use enshwick::pacing;

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    load.on_server_wait(|wait_ns| {
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
[the life of a mod](concepts/lifecycle.md)); on the client, and with both caps 0, it returns
`false`. The game's log shows the result, `[task] Group 'Default'
Worker:…`.

```rust
use enshwick::pacing::TaskWorkers;

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    if !load.cap_task_workers(TaskWorkers::at_most(4))? {
        enshwick::info!("the task workers stay as the game sizes them");
    }
    Ok(())
}
```

[`SERVER_IDLE_WAIT_NS`]: https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/pacing.rs
[`SERVER_ONLINE_WAIT_NS`]: https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/pacing.rs
[`SERVER_SLOW_WAIT_NS`]: https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/pacing.rs
