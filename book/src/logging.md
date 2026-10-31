# Logging and diagnostics

The loader writes one set of logs for itself and every mod, keeps a status
summary of what runs, and turns a failing mod off with a reason in the log.
This chapter covers what a mod writes there and where to look.

## Log macros

`enshwick::error!`, `warn!`, `info!` and `debug!` write a line under the
mod's id, formatted like `format!`:

```rust
enshwick::info!("{} altars merged", 3);
enshwick::debug!("bed {} checked", 1210);
```

For lines that may repeat:

| Macro                                                                       | Logs                                                                                           |
| --------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------- |
| `info_once!`, `warn_once!`, `error_once!`                                   | the first time this call site runs in the mod's life, never again                              |
| `info_every!(period, ...)`, `warn_every!(period, ...)`, `error_every!(...)` | at most once per `period` from this call site, adding how many lines it skipped since the last |

```rust
use std::time::Duration;

for bed in 0..100 {
    enshwick::warn_every!(Duration::from_secs(60), "bed {bed} has no owner");
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
| `report.md`     | the report a player attaches to an issue            |

A line reads `hh:mm:ss.mmmZ level [source] message`, in UTC. At start the
files of the two sessions before are kept as `<name>.1.log` (the last) and
`<name>.2.log`, each cut to its last 8 MiB, and older ones are deleted. A background thread writes them and
flushes every line, so a hook never waits on the disk and the last lines
before a crash are kept. Up to 4096 lines wait for it; a line that finds the
queue full is dropped rather than making the game wait, and the next line
written says how many were dropped.

`report.md` gathers what an issue needs in one file: the loader version and
the stub's contract, Windows or the Wine version with `WINEDLLOVERRIDES`, and
the status summary. It lists the DLLs in `mods/`, in `mods/chain/` and other
mods' proxies beside the game, with their sizes, and the latest 200 warnings
and errors of the loader and every mod. The loader writes it once its log
opens, again when it cannot serve the game, and with every status summary,
each time after the log has taken the lines before it. Paths show the game's
folder as `<game>`, the user's profile folder as `%USERPROFILE%` and the name
in a Linux home folder as `<user>`, in any case and with either slash.

### Reading the log from the console

The writer keeps the latest 1024 lines in memory. The
[console](chat.md#the-console) reads them with
`enshwick log [mod] [level] [n]`, the words in any order:

- `mod`: one mod's lines, or the loader's with `enshwick`; every source when
  left out;
- `level`: `error`, `warn`, `info` or `debug`, the lines at that level or more
  severe; every kept line when left out;
- `n`: how many of the latest lines, 10 by default and at most 16.

`enshwick log you.trading-post warn 5` shows the last five warnings and errors of
`you.trading-post`. Only lines the [levels](#levels) keep are there to read.

## Levels

The player picks how much is kept in `mod-configs/enshwick.toml`, the loader's
own settings:

```toml
# error, warn, info or debug; info by default
log_level = "info"
# The same per mod id, over log_level; "enshwick" is the loader itself.
log_levels = { "you.lockers" = "debug" }
```

The loader watches this file like a mod's settings: a change applies about a
second later, and every mod's level follows without a restart.

The keys of `mod-configs/enshwick.toml`:

| Key                  | Default   | Meaning                                                                                                                 |
| -------------------- | --------- | ----------------------------------------------------------------------------------------------------------------------- |
| `log_level`          | `"info"`  | the most detailed lines kept                                                                                            |
| `log_levels`         | empty     | `log_level` per mod id                                                                                                  |
| `status_minutes`     | `10`      | minutes between status summaries; `0` writes one only on request                                                        |
| `diagnostics`        | `false`   | count the calls of every hooked function; read at start                                                                 |
| `diagnostics_timing` | `false`   | with `diagnostics`, also time every 256th call; read at start                                                           |
| `migrations_dry_run` | `false`   | only log what mods' [migrations](migrations.md) would change                                                            |
| `build_patterns`     | `true`    | run a build without its table on patterns; read at start                                                                |
| `dev_reload`         | `false`   | reload changed mods that allow it, on the server or the client; read at start                                           |
| `knowledge_guard`    | `"watch"` | watch, enforce or turn off the [knowledge guard](knowledge.md#what-clients-ask-for); read at start                      |
| `require_enshwick`   | `true`    | kick players whose game lacks the [join mods](concepts/join-check.md); read at start                                    |
| `join_hello_seconds` | `30`      | seconds a joining player's game has to name its join mods; read at start                                                |
| `join_note_seconds`  | `10`      | seconds between the chat line and the disconnect; read at start                                                         |
| `allowlist`          | `false`   | on a dedicated server, let in only the [allowlist](world.md#the-allowlist)'s accounts and administrators; read at start |
| `language`           | `""`      | on a dedicated server, language code of the console's and other texts no player's game names; empty for English         |
| `motd`               | `""`      | the [message of the day](https://enshwick.com/guides/run-a-server-in-docker/#greet-players-when-they-join) players read in chat; at most 4 lines and 500 bytes |

The file rejects unknown keys. A file that does not parse at start is logged
and the loader runs with the defaults; a change that does not parse is logged
and skipped.

## Status summary

The loader writes a status summary to `logs/enshwick/status.log`: a minute
after start, then every `status_minutes`, and whenever the console runs
`enshwick status`. Each write replaces the last. It lists:

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

`enshwick status` also answers the console with a short form of it.

A mod adds its own lines with
[`load.add_status(|| ...)`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/status.rs).
The closure returns a `String`; each non-empty line shows under the mod. It
runs only when the summary is written, on a loader thread or the game thread,
never inside a hook, and only while the mod runs. Keep what it reports in
atomics and read them there:

```rust
use std::sync::atomic::{AtomicU64, Ordering};

static MERGED: AtomicU64 = AtomicU64::new(0);

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    load.add_status(|| format!("{} zones merged", MERGED.load(Ordering::Relaxed)))
}
```

## Features that turn off alone

A mod made of parts that can fail apart wraps each part's setup in
[`load.feature(name, |load| ...)`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/status.rs).
When the closure returns an `Off`, only that part is off: the mod keeps
running, its log names the part and the reason, and the status summary and
the Mods screen show it. The closure's value comes back as `Some` when the
part runs. Look up everything the part needs before it links hooks or adds
content, since what it registered before it failed stays:

```rust
fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    if let Some(glow) = load.feature("glow", |load| load.symbol("lantern.glow")) {
        let _ = glow;
    }
    Ok(())
}
```

A reason made with `Off::text(key, args)` names a text of the mod's
[texts](content/texts.md). The Mods screen, the status summary and the log
show it in the reader's language, with each `{name}` filled from `args`. A
key with no text shows as the key and its values.

```rust
fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let _texts = load.texts("[en]\n\"glow.off\" = \"No glow: {symbol} is missing.\"\n")?;
    let glow = load.feature("glow", |load| {
        load.symbol("lantern.glow")
            .map_err(|_| enshwick::Off::text("glow.off", &[("symbol", "lantern.glow")]))
    });
    let _ = glow;
    Ok(())
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
mod: event, game event and mod event handlers, actions, messages, interface
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
  you.lanterns 52.9 µs/s: progress filters 51.8 µs/s in 366110 calls; events 1.1 µs/s in 1 calls
```

The console asks for the five costliest with `enshwick profile`, and starts
counting anew with `enshwick profile reset`, such as before trying something
in game.

## Conflicts

The summary and `enshwick conflicts` on the console list where mods meet:

| Line | Means |
| --- | --- |
| `hook <function>: a (priority 0) then b (priority 5)` | both link the function, in that running order, each calling on |
| `hook <function>: ...; b refused, a holds it exclusively` | `a` linked it with `EXCLUSIVE`, and `b`'s link was refused |
| `code patch at <address>: a holds it; refused: b` | `b` asked to [patch](code-patches.md) bytes `a` patches already |
| `<filter>: a, b, chained in load order` | both filter it, such as interactions or damage; each may refuse |
| `skill override: a, b, asked in load order` | the first answer decides |
| `<feature>: a holds it; refused: b` | one mod at a time, such as the altar limit |
| `the item registry: patched by a then b` | both change one resource, each patch on the one before |
| `max_stack of <item> is 999 from b, which overrides 100 from a` | both set one [vanilla value](content/packs.md); the later in load order wins |
| `component <type>: written by a, b, the last write wins` | both [write](world.md#writing-components) that component of entities |
| `point <name>: a then b, each may change the answer` | both extend one [extension point](extending.md), in that order |
| `...; b changed the answer a had set, 3 times` | `b` overturned `a`'s answer that often since start |
| `game code at <place>, 5 bytes: changed outside the API by a.dll while it loaded` | see below |

`conflicts: none` means no two mods share anything.

### Code changed outside the API

A mod that writes game code itself, with a hooking library of its own or by
hand, passes none of the checks above. The loader finds such changes anyway:
it keeps a copy of the game's code section while it starts and compares it
after each DLL in `mods/` loads, after each mod's `init` and after each
`ready`. For a change it finds, the loader names the DLL or mod that ran last.
It logs the change as a warning in the loader's log and lists it with the
conflicts:

| Line ends with | The change came |
| --- | --- |
| `by another program before Enshwick started` | before the loader, from a DLL in `mods/chain/` or another program |
| `by a.dll while it loaded` | while that DLL loaded, from its own start-up code |
| `by another program while Enshwick unloaded the DLLs it skips` | while the loader unloaded a mod that is off or an older copy |
| `by you.mod in its init` or `in its ready` | while that mod's `init` or `ready` ran |
| `by another program after every init, before Enshwick turned on its hooks` | between the last `init` and arming, from a thread of some mod or another program |

The copy starts from the executable on disk, so code changed before the
loader started shows too; when the file's code does not read as the code in
memory, as for an encrypted executable, the copy starts when the loader does.
Two names on one line mean two mods wrote the same bytes, and the later one
won.

The loader's own writes are never listed: its hooks and the code patches
mods ask for through the API, and the jump that holds the server's entry
point until the loader is ready. For a thread's write, the loader names
whatever ran last before the next comparison. Changes made after the last `ready`,
and changes to game data rather than code, are not seen.

## Faults

When the loader turns a mod off, `loader.log` and the mod's log say why, the
mod's log with an error `off: <reason>`, and the Mods screen shows the reason under the mod, in the
reader's language for an `Off::text`. The summary's list of mods turned off
keeps the reason, and `enshwick` on the console marks the mod `(off)`. A
hardware fault that ends the game goes to `crash.log` instead, and the next
start shows its line in the log and at the top of `status.log`.

What turns a mod off, and the reason each case logs, is in
[When a mod fails](concepts/lifecycle.md#when-a-mod-fails).
