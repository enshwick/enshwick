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

| Macro                                                   | Logs                                                                                           |
| ------------------------------------------------------- | ---------------------------------------------------------------------------------------------- |
| `info_once!`, `warn_once!`                              | the first time this call site runs, never again                                                |
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

| File            | Holds                                               |
| --------------- | --------------------------------------------------- |
| `all.log`       | every line, the loader's and every mod's            |
| `loader.log`    | the loader's own lines                              |
| `mods/<id>.log` | one mod's lines                                     |
| `status.log`    | the latest [status summary](#status-summary)        |
| `crash.log`     | each crash, blamed on a mod, the loader or the game |

A line reads `hh:mm:ss.mmmZ level [source] message`, in UTC. At start the
files of the two sessions before are kept as `<name>.1.log` (the last) and
`<name>.2.log`, each cut to its last 8 MiB, and older ones are deleted. A background thread writes them and
flushes every line, so a hook never waits on the disk and the last lines
before a crash are kept. Up to 4096 lines wait for it; a line that finds the
queue full is dropped rather than making the game wait, and the next line
written says how many were dropped.

### Reading the log in game

The writer keeps the latest 1024 lines in memory. An administrator reads them
in chat with `/enshwick log [mod] [level] [n]`, the words in any order:

- `mod`: one mod's lines, or the loader's with `enshwick`; every source when
  left out;
- `level`: `error`, `warn`, `info` or `debug`, the lines at that level or more
  severe; every kept line when left out;
- `n`: how many of the latest lines, 10 by default and at most 16.

`/enshwick log safes warn 5` shows the last five warnings and errors of
`safes`. Only lines the [levels](#levels) keep are there to read.

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

| Key                  | Default   | Meaning                                                                                            |
| -------------------- | --------- | -------------------------------------------------------------------------------------------------- |
| `log_level`          | `"info"`  | the most detailed lines kept                                                                       |
| `log_levels`         | empty     | `log_level` per mod id                                                                             |
| `status_minutes`     | `10`      | minutes between status summaries; `0` writes one only on request                                   |
| `diagnostics`        | `false`   | count the calls of every hooked function; read at start                                            |
| `diagnostics_timing` | `false`   | with `diagnostics`, also time every 256th call; read at start                                      |
| `migrations_dry_run` | `false`   | only log what mods' [migrations](migrations.md) would change                                       |
| `build_patterns`     | `true`    | run a build without its table on patterns; read at start                                           |
| `dev_reload`         | `false`   | reload changed mods that allow it, on the server or the client; read at start                      |
| `knowledge_guard`    | `"watch"` | watch, enforce or turn off the [knowledge guard](knowledge.md#what-clients-ask-for); read at start |

The file rejects unknown keys. A file that does not parse at start is logged
and the loader runs with the defaults; a change that does not parse is logged
and skipped.

## Status summary

The loader writes a status summary to `logs/enshwick/status.log`: a minute
after start, then every `status_minutes`, and whenever an administrator types
`/enshwick status`. Each write replaces the last. It lists:

- the loader and game versions, and whether this process hosts a game;
- every mod with its name, version and whether it runs, its authors, licence
  and homepage, notes such as a recommended mod that is missing, and the lines
  the mod adds;
- the mods turned off, and why;
- what mods serve and patch, every service, mod event and extension point
  with the mods on it, and the links left inert with the reason;
- the number of hooked functions, and with `diagnostics` their calls;
- with `diagnostics`, what each mod costs ([Per-mod costs](#per-mod-costs));
- where mods meet ([Conflicts](#conflicts)).

`/enshwick status` also answers the administrator in the chat with a short
form of it.

A mod adds its own lines with
[`load.add_status(|| ...)`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/status.rs).
The closure returns a `String`; each non-empty line shows under the mod. It
runs only when the summary is written, on a loader thread or the game thread,
never inside a hook, and only while the mod runs. Keep what it reports in
atomics and read them there:

```rust
use std::sync::atomic::{AtomicU64, Ordering};

static MERGED: AtomicU64 = AtomicU64::new(0);

fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    load.add_status(|| format!("{} zones merged", MERGED.load(Ordering::Relaxed)))
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

## Per-mod costs

With `diagnostics = true` the loader also times every call it makes into a
mod: event, game event and mod event handlers, commands, messages, interface
frames, filters and vetoes, survivor and altar callbacks, stat changes,
inventory results, status lines, resource patches, migrations and links on
extension points, each link apart. A
callback's time leaves out the callbacks it causes in other mods, which count
for those. With `diagnostics_timing = true` as well, every link and every
original has a timed thunk in front of it, and a link's own time is its
thunk's less the rest of the chain's, so a mod pays only for its own link
even when several mods hook one function. Timing a callback costs about
14 ns; with `diagnostics` off it costs nothing measurable.

The summary lists the ten costliest mods, each with its work per second of
time and its four costliest items; the loader's own links count under
`enshwick`, with the callbacks they make into mods inside them:

```text
per-mod costs over 1m 00s (since start), work per second of time:
  enshwick 0.92 ms/s: hook comfort.base_register 0.39 ms/s in 366110 calls; hook server.post_step 0.18 ms/s in 1456 calls
  furniture-comfort 52.9 µs/s: progress filters 51.8 µs/s in 366110 calls; events 1.1 µs/s in 1 calls
```

An administrator asks for the five costliest with `/enshwick profile`, and
starts counting anew with `/enshwick profile reset`, such as before trying
something in game.

## Conflicts

The summary and `/enshwick conflicts` (administrators) list where mods meet:

| Line                                                      | Means                                                           |
| --------------------------------------------------------- | --------------------------------------------------------------- |
| `hook <function>: a (priority 0) then b (priority 5)`     | both link the function, in that running order, each calling on  |
| `hook <function>: ...; b refused, a holds it exclusively` | `a` linked it with `EXCLUSIVE`, and `b`'s link was refused      |
| `<filter>: a, b, chained in load order`                   | both filter it, such as interactions or damage; each may refuse |
| `skill override: a, b, asked in load order`               | the first answer decides                                        |
| `<feature>: a holds it; refused: b`                       | one mod at a time, such as the altar limit or a command name    |
| `the item registry: patched by a then b`                  | both change one resource, each patch on the one before          |
| `point <name>: a then b, each may change the answer`      | both extend one [extension point](extending.md), in that order  |
| `...; b changed the answer a had set, 3 times`            | `b` overturned `a`'s answer that often since start              |

`conflicts: none` means no two mods share anything.

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
and cannot be caught. When it ends the game, the loader writes `crash.log`,
naming the mod whose code faulted, whose callback it was running, or whose
code was on the stack, before the game's own crash handling runs; the next
start shows the line in its log and at the top of `status.log`. See
[What is in crash.log?](faq.md#what-is-in-crashlog).
