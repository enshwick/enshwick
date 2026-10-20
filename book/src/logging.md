# Logging and diagnostics

The loader writes one set of logs for itself and every mod, keeps a status
summary of what runs, and turns a failing mod off with a reason in the log.
This chapter covers what a mod writes there and where to look.

## Log macros

`enshwick_loader_api::error!`, `warn!`, `info!` and `debug!` write a line under the
mod's id, formatted like `format!`:

```rust
enshwick_loader_api::info!("{} altars merged", 3);
enshwick_loader_api::debug!("bed {} checked", 1210);
```

For lines that may repeat:

| Macro | Logs |
| --- | --- |
| `info_once!`, `warn_once!` | the first time this call site runs, never again |
| `warn_every!(period, ...)`, `error_every!(period, ...)` | at most once per `period` from this call site, adding how many lines it skipped since the last |

```rust
use std::time::Duration;

for bed in 0..100 {
    enshwick_loader_api::warn_every!(Duration::from_secs(60), "bed {bed} has no owner");
}
```

A line above the mod's level costs one atomic load: its arguments are neither
evaluated nor formatted. Log milestones and failures, not every call of a
hook; a line that is kept is formatted on the calling thread.

## Log files

The loader writes to `logs/enshwick/` beside the game:

| File | Holds |
| --- | --- |
| `all.log` | every line, the loader's and every mod's |
| `loader.log` | the loader's own lines |
| `mods/<id>.log` | one mod's lines |
| `status.log` | the latest [status summary](#status-summary) |
| `crash.log` | which mod's code was on the stack when the game crashed |

A line reads `hh:mm:ss.mmmZ level [source] message`, in UTC. The files of the
previous session are truncated at start. A background thread writes them and
flushes every line, so a hook never waits on the disk and the last lines
before a crash are kept.

## Levels

The player picks how much is kept in `mod-configs/enshwick.toml`, the loader's
own settings:

```toml
# error, warn, info or debug; info by default
log_level = "info"
# The same per mod id, over log_level; "enshwick" is the loader itself.
log_levels = { flame-altar-zones = "debug" }
```

The loader watches this file like a mod's settings: a change applies about a
second later, and every mod's level follows without a restart.

The keys of `mod-configs/enshwick.toml`:

| Key | Default | Meaning |
| --- | --- | --- |
| `log_level` | `"info"` | the most detailed lines kept |
| `log_levels` | empty | `log_level` per mod id |
| `status_minutes` | `10` | minutes between status summaries; `0` writes one only on request |
| `diagnostics` | `false` | count the calls of every hooked function; read at start |
| `diagnostics_timing` | `false` | with `diagnostics`, also time every 256th call; read at start |

The file rejects unknown keys. A file that does not parse at start is logged
and the loader runs with the defaults; a change that does not parse is logged
and skipped.

## Status summary

The loader writes a status summary to `logs/enshwick/status.log`: a minute
after start, then every `status_minutes`, and whenever an administrator types
`/enshwick status`. Each write replaces the last. It lists:

- the loader and game versions, and whether this process hosts a game;
- every mod with its version and whether it runs, with the lines the mod adds;
- the mods turned off, and why;
- what mods serve and patch;
- the number of hooked functions, and with `diagnostics` their calls.

`/enshwick status` also answers the administrator in the chat with a short
form of it.

A mod adds its own lines with
[`load.status(|| ...)`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/status.rs).
The closure returns a `String`; each non-empty line shows under the mod. It
runs only when the summary is written, on a loader thread or the game thread,
never inside a hook, and only while the mod runs. Keep what it reports in
atomics and read them there:

```rust
use std::sync::atomic::{AtomicU64, Ordering};

static MERGED: AtomicU64 = AtomicU64::new(0);

fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    load.status(|| format!("{} zones merged", MERGED.load(Ordering::Relaxed)))
}
```

## Hook diagnostics

With `diagnostics = true` the loader counts the calls of every hooked
function, one locked increment in its thunk, and the summary lists the 40
busiest, with the mods linked on each. Add `diagnostics_timing = true` and
every 256th call is timed as well; the summary shows the mean time of the
whole chain with the original.

Both are read at start, since the thunks are written once. Without them the
thunks count nothing and a hooked call runs as before.

## Faults

A mod is turned off when:

- `init` returns `Err(Off)`: the mod's log has a warning `off: <reason>`;
- `init` panics: an error `off: init panicked`;
- a hook, event handler, command handler, status closure or `ready` panics
  later: an error `off: <what panicked>`.

The panic is caught at the boundary, the loader unlinks every hook of the mod,
and the game goes on vanilla where the mod acted. Its event handlers, commands
and status lines stop with it. The summary's list of mods turned off keeps the
reason, and `/enshwick` marks the mod `(off)`.

A hardware fault, such as an access violation in a mod's code, is not a panic
and cannot be caught. The loader then writes `crash.log`, naming the mod whose
code was on the stack, before the game's own crash handling runs.
