<!--
type: how-to
description: The loader writes one log folder for itself and every mod, a status summary of what runs, and turns a failing mod off with the reason in the log.
-->

# Logging and diagnostics

The loader writes one log folder for itself and every mod. On request, it
writes a status summary of what runs. It turns a failing mod off with a reason
in the log.

## Log macros

Four macros write a line under the mod's id, formatted like `format!`:

- `enshwick::error!`
- `warn!`
- `info!`
- `debug!`

In Rust and Luau:

```rust
enshwick::info!("{} altars merged", 3);
enshwick::debug!("bed {} checked", 1210);
```

```luau
enshwick.log.info(`{3} altars merged`)
print("bed 1210 checked") -- as enshwick.log.info
```

In Luau, `enshwick.log` has these functions:

- `info`
- `warn`
- `error`
- `level`, which reads the mod's level.

The macros that log once or at most once a period are Rust's.

For lines that may repeat:

| Macro | Logs |
| --- | --- |
| `info_once!` | the first time this call site runs in the mod's life, never again |
| `warn_once!` | as `info_once!` |
| `error_once!` | as `info_once!` |
| `info_every!(period, ...)` | at most once per `period` from this call site, adding how many lines it skipped since the last |
| `warn_every!(period, ...)` | as `info_every!` |
| `error_every!(...)` | as `info_every!` |

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

The loader writes every log to `logs/` beside the game, one file per source
and run:

| File                                    | Holds                                                    |
| --------------------------------------- | -------------------------------------------------------- |
| `enshwick_<date>_<time>.log`            | the loader's own lines of the run that started then      |
| `<mod id>_<date>_<time>.log`            | one mod's lines of that run                              |
| `enshwick-crash_<date>_<time>.log`      | a crash, blamed on a mod, the loader or the game         |
| `start.log`                             | lines from before the loader started, while it never did |

The time is the run's start in UTC, such as `enshwick_2026-11-03_10-00-00.log`.
A line reads `hh:mm:ss.mmmZ level [source] message`, in UTC. Each source keeps
its newest 10 files and deletes older ones as it opens a new one; deleting any
file by hand is safe. A file that would pass 8 MiB ends with a line naming the
next file, and the run goes on there. A background thread writes them and
flushes every line. So a hook never waits on the disk, and the last lines
before a crash are kept. Up to 4096 lines wait for it. A line that finds the
queue full is dropped rather than making the game wait. The next line written
says how many were dropped.

The stub and the loader write the lines from before the log opens to
`start.log`. The loader's file takes them in as it opens and removes
`start.log`. So `start.log` stays only when the loader did not start. When the game's folder
takes no files, they go to `enshwick/start.log` in the Windows temporary
folder. The loader's file starts with its version, Windows or the Wine
version, and the number of logical processors.

### Reading the log from the console

The writer keeps the latest 1024 lines in memory. The
[console](chat.md#the-console) reads them with
`enshwick log [mod] [level] [n]`, the words in any order:

- `mod`: one mod's lines, or the loader's with `enshwick`; every source when
  left out;
- `level`: one of the [levels](#levels), for the lines at that level or more
  severe; every kept line when left out;
- `n`: how many of the latest lines, 10 by default and at most 16.

`enshwick log you.trading-post warn 5` shows the last five warnings and errors of
`you.trading-post`. Only lines the [levels](#levels) keep are there to read.

## Levels

The player picks how much is kept in `enshwick.toml` in the game's folder,
the loader's own settings. The loader writes the file on first start with
every key commented out at its default; remove the `#` before a key to set it:

```toml
# error, warn, info or debug; info by default
log_level = "info"
# The same per mod id, over log_level; "enshwick" is the loader itself.
log_levels = { "you.lockers" = "debug" }
```

The loader watches this file: a change of the levels applies about a second
later, and every mod's level follows without a restart.

The keys of `enshwick.toml`:

| Key                  | Default  | Meaning                                                       |
| -------------------- | -------- | ------------------------------------------------------------- |
| `log_level`          | `"info"` | the most detailed lines kept                                  |
| `log_levels`         | empty    | `log_level` per mod id                                        |
| `diagnostics`        | `false`  | count the calls of every hooked function; read at start       |
| `diagnostics_timing` | `false`  | with `diagnostics`, also time every 256th call; read at start |
| `build_patterns`     | `true`   | run a build no compatibility report covers; read at start     |
| `release_check`      | `true`   | ask GitHub once per start for a newer release; read at start  |

With `release_check` on, the loader asks GitHub once per start. A release
asks for the Enshwick tags, and a prerelease for the newest releases. It asks on a thread of its own, with a 5-second
limit per step. A newer release shows as one dimmed line on the Mods screen
with its version and the address of its page. The loader downloads and installs
nothing. A dedicated server only logs it. A prerelease counts only while the
running loader is one. No network, or any other failure, shows nothing and
logs one line.

A key whose value does not read keeps its default, and the log names it with
error [E1007](https://enshwick.com/errors/E1007); every other key applies. A
file that does not parse reads as the defaults. The rule a world sets for
joining players, `require_enshwick`, lives with the world: see
[The join check](concepts/join-check.md).

## Status summary

The console's `enshwick status` writes a status summary into the loader's
log. It lists:

- the loader and game versions, and whether this process hosts a game;
- every mod with its name, version and whether it runs. Each also shows its
  authors, licence and homepage, its notes and the lines the mod adds. A note
  may say that a recommended mod is missing;
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
runs only when the summary is written, and only while the mod runs. It runs on
a loader thread or the game thread, never inside a hook. Keep what it reports in
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
When the closure returns an `Off`, only that part is off. The mod keeps
running, and its log names the part and the reason. The status summary and
the Mods screen show it too. The closure's value comes back as `Some` when the
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
    let _texts = load.texts_from("[en]\n\"glow.off\" = \"No glow: {symbol} is missing.\"\n")?;
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
function, one locked increment in its thunk. The summary lists the 40
busiest, with the mods linked on each. Add `diagnostics_timing = true` and
every 256th call is timed as well; the summary shows the mean time of the
whole chain with the original.

Both are read at start, since the thunks are written once. Without them the
thunks count nothing and a hooked call runs as before.

## Per-mod costs

With `diagnostics = true` the loader also times every call it makes into a
mod. These are the calls it times:

- event, game event and mod event handlers;
- actions and messages;
- interface frames;
- filters and vetoes;
- survivor and altar callbacks;
- stat changes and inventory results;
- status lines and resource patches;
- links on extension points, each link apart.

A callback's time leaves out the callbacks it causes in other mods, which
count for those. With `diagnostics_timing = true` as well, every link and
every original has a timed thunk in front of it. A link's own time is its
thunk's less the rest of the chain's. So a mod pays only for its own link,
even when several mods hook one function. Timing a callback costs about
14 ns; with `diagnostics` off it costs nothing measurable.

The summary lists the ten costliest mods, each with its work per second of
time and its four costliest items. The loader's own links count under
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
| `hook <function>: ...; b refused, a holds it exclusively` | `a` linked it with `EXCLUSIVE`. The link of `b` was refused |
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
hand, passes none of the checks above. The loader finds such changes anyway.
It keeps a copy of the game's code section while it starts. It compares the
copy after each DLL in `mods/` loads and after each mod's `init`. It also
compares it after each `ready`. A change found after a DLL or mod ran is put
down to it only when the changed code jumps into that DLL. The jump may be
direct or go through up to three more jumps, as a hook does. Any other change
is put down to another program, whose threads may write at any time. The
loader logs the change as a warning in the loader's log and lists it with the
conflicts:

| Line ends with | The change came |
| --- | --- |
| `by another program before Enshwick started` | before the loader, from another mod's proxy or another program |
| `by a.dll while it loaded` | while that DLL loaded, from its own start-up code, jumping into it |
| `by another program while Enshwick unloaded the DLLs it skips` | while the loader unloaded a mod that is off or an older copy |
| `by you.mod in its init` or `in its ready` | while that mod's `init` or `ready` ran |
| `by another program` | while a DLL loaded or a mod ran `init` or `ready`, without a jump into it: from another program's thread |
| `by another program after every init, before Enshwick turned on its hooks` | between the last `init` and arming, from a thread of some mod or another program |

The copy starts from the executable on disk, so code changed before the
loader started shows too. When the file's code does not read as the code in
memory, as for an encrypted executable, the copy starts when the loader does.
Two names on one line mean two mods wrote the same bytes, and the later one
won.

The loader's own writes are never listed. They are its hooks and the code
patches mods ask for through the API. The jump that holds the server's entry
point until the loader is ready is one too. Changes made after the last `ready`,
and changes to game data rather than code, are not seen.

## Faults

When the loader turns a mod off, the loader's log and the mod's log say why.
The mod's log says it with an error `off: <reason>`. The Mods screen shows the
reason under the mod, in the reader's language for an `Off::text`. The
summary's list of mods turned off keeps the reason, and `enshwick` on the
console marks the mod `(off)`. A hardware fault that ends the game goes to the
loader's log and to a crash file of its own,
`logs/enshwick-crash_<date>_<time>.log`. The next start shows its line in
the log and at the top of the status summary.

What turns a mod off, and the reason each case logs, is in
[When a mod fails](concepts/lifecycle.md#when-a-mod-fails).

## Worked example

[`enshwick-optimizer`](https://github.com/enshwick/mods/tree/main/optimizer)
adds its lines to the status summary with `add_status`. It does so in
[`lib.rs`](https://github.com/enshwick/mods/blob/main/optimizer/src/lib.rs).
