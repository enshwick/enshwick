# FAQ and troubleshooting

## Where are the logs?

Everything is under `logs/enshwick/` in the game's folder (beside
`enshrouded.exe` or `enshrouded_server.exe`):

| File            | What it holds                                         |
| --------------- | ----------------------------------------------------- |
| `all.log`       | Every line, from the loader and every mod             |
| `loader.log`    | The loader's lines                                    |
| `mods/<id>.log` | One mod's lines                                       |
| `status.log`    | The latest status summary                             |
| `crash.log`     | Crashes, each blamed on a mod, the loader or the game |

`all.log`, `loader.log` and the mod logs start empty each session; the two
sessions before stay as `all.1.log` and `all.2.log` (and so on for each
file), at most 8 MiB each. Each line
names its time (UTC), level and source:

```text
01:02:03.004Z warn  [you.lockers] merged 3 altars
```

How much is kept is set in `mod-configs/enshwick.toml`, for all sources and
per mod id (`enshwick` is the loader); see
[Logging and diagnostics](logging.md):

```toml
log_level = "info"
log_levels = { "you.lockers" = "debug" }
```

## What is in crash.log?

A line for each crash: a hardware fault nothing handled, such as an access
violation, an illegal or privileged instruction, an integer division by zero
or a stack overflow, that ends the game. Faults the game or a mod handles and
survives are never written. The loader writes the line as the game's own
crash handler starts writing its crash dump, or when no handler is left, and
the game's crash handling then runs as it would without Enshwick:

```text
2026-10-21 23:45:18Z fault 0xc0000005 at 0x7ffb1a2b3c4d, blamed on you.trading-post: in its code (+0x3c4d in you.trading-post); reading 0x0
```

The parts are the time, the exception code, the address, and whom the fault
is blamed on:

| Blame                                        | Means                                                                     |
| -------------------------------------------- | ------------------------------------------------------------------------- |
| `blamed on <mod>: in its code`               | the fault is in the mod's DLL                                             |
| `blamed on <mod>: in its <feature> callback` | the loader was running one of the mod's callbacks, such as an action      |
| `blamed on <mod>: its code is on the stack`  | the fault is in game code a mod's hook or call led to, found by unwinding |
| `in the loader`                              | the fault is in the loader with no mod's callback running or code near it |
| `in the game: no mod's code on the stack`    | Enshwick and its mods are not involved                                    |

The offset in brackets is the address inside the DLL that holds it, and the
last part is what the faulting access read, wrote or ran.

`crash.log` is appended to across sessions, unlike the other logs. The next
start logs the last crash as a warning and shows it at the top of
`status.log`; the line waits in `mod-data/enshwick/last-crash.txt` until
then.

## What is in status.log?

A summary the loader writes a minute after start, then every
`status_minutes` (10 by default, 0 for on demand only), and when an
console runs `enshwick status`. Each write replaces the last. It lists
the game build, every mod with its version and whether it runs, the lines each
running mod adds with `load.add_status`, the mods turned off and why, what mods
serve and patch, and the hooked functions. With `diagnostics = true` in
`mod-configs/enshwick.toml` it also counts the calls of every hooked function.

## Why is my mod off?

Look in `loader.log` for the mod's DLL name or id, and in `mods/<id>.log`. A
mod that never initialised was skipped; a mod that initialised and stopped
was turned off. A turned-off mod's log has a line `off: <reason>` at `error`;
when the mod itself returned `Err(Off)` from `init`, the line before it, at
`warn`, carries the mod's own reason.

### Nothing loads at all

| Message in `loader.log`                                                                         | What it means                                                                         |
| ----------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------- |
| `unknown game build <guid>/<age>; the game runs vanilla until Enshwick has its build table`     | The game was updated and `build_patterns = false` in `mod-configs/enshwick.toml`      |
| `<name> is not where the build table says; nothing loads`                                       | The build looks known but does not match; treat it as unknown                         |
| `this is not Enshrouded or its dedicated server; nothing loads`                                 | `powrprof.dll` was loaded by another program                                          |

On an unknown build every mod's hold applies; see
[The game stops before the world loads](#the-game-stops-before-the-world-loads).

### Some features are off after a game update

`loader.log` reads `game build <guid>/<age> has no build table yet: <n>
entries found by their code patterns, <m> missing; ...`, and at `debug` the
missing entries. The game was updated, and
the loader runs it on the code patterns of the newest build table it has.
Whatever those patterns do not find stays off: a loader feature logs
`<feature> off: not in this build`, and a mod that needs a missing entry turns
off with `symbol <id>: missing on this game build`. Everything else runs
until a loader build with the new build's table replaces this one.
`cargo run -p enshwick-compat -- <exe> --mods <dir>`, in a checkout of the
[platform repository](https://github.com/enshwick/enshwick), shows the
same verdicts without starting the game.

### The mod was skipped

These lines read `<file> skipped: <reason>`, or name the file alone:

| Message                                                                                                         | What it means                                                          |
| --------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------- |
| `<file> is not an Enshwick mod`                                                                                 | The DLL lacks the four required exports of `declare_mod!`              |
| `<file> could not be loaded`                                                                                    | Windows could not load the DLL, such as a missing dependency           |
| `<file> is built for ABI 2, the loader speaks 1`                                                                | The mod was built for another ABI major                                |
| `skipped: not for this side of the game`                                                                        | A client-only mod on the dedicated server                              |
| `old.dll skipped: you.trading-post 0.1.0 is also in trading_post.dll, which runs 0.2.0; old.dll can be deleted` | Two files hold one mod; the highest version runs, then the newest file |
| `skipped: requires trading-post, which is not loaded`                                                           | A required mod is absent or was skipped itself                         |
| `skipped: requires trading-post ^0.2, found 0.1.3`                                                              | A required mod is too old or too new                                   |
| `skipped: does not work beside old-market 0.4.0; remove one of them`                                            | The mod declares `breaks` on a loaded mod, and gives way               |
| `skipped: in a dependency cycle`                                                                                | Mods require each other in a circle                                    |
| `skipped: the id "My Mod" is not <author>.<name>, each of [a-z0-9-]`                                            | The manifest's id is invalid; other manifest errors read the same way  |

The client loads server mods too, since it runs the server's code while it
hosts a game; see [Sides and hosting](concepts/sides-and-hosting.md).

### The mod turned itself off in init

The mod's log has `off: <reason>` at `warn`, then the loader's
`off: init returned 3`. Common reasons, as the safe layer words them:

| Reason                                                 | What it means                                                          |
| ------------------------------------------------------ | ---------------------------------------------------------------------- |
| `symbol quest.reward: missing on this game build`      | Neither the mod nor the loader pins the symbol for this build          |
| `constant ecs.context_row: missing on this game build` | The same for an engine constant                                        |
| `settings: <parse error>`                              | `mod-configs/<id>.toml` does not parse into the settings type          |
| `hook REWARD: refused by the loader`                   | Another mod holds the function exclusively, or the mod hooked it twice |
| `service <name> v<major>: <reason>`                    | No running mod publishes it, or its table lacks or changed a field     |

A mod built for a newer ABI minor than the loader's never starts: the log
shows `built for ABI 2.<n>, newer than the loader's 2.<m>: update Enshwick`.
A settings file that stops parsing while the game runs does
not turn the mod off: the mod logs
`settings change skipped, the running values stay: ...` and keeps its values.

### The mod was turned off later

| `off:` reason                                                  | What happened                                                                     |
| -------------------------------------------------------------- | --------------------------------------------------------------------------------- |
| `init returned 6` after `off: init panicked`                   | `Mod::init` panicked                                                              |
| `its hook at 0x... could not be armed`                         | The loader could not detour the function; `hook at 0x... not armed: ...` says why |
| `its code patch at 0x... was skipped`                          | The bytes no longer read as expected, or overlap a hook                           |
| `panic in hook REWARD`                                         | A detour panicked; the call went on to the original                               |
| `panic in hook REWARD after next`                              | Work in `REWARD.after_next` panicked; the call kept what `next` gave              |
| `panic in the hook at 0x...`                                   | A mid-function hook's handler panicked                                            |
| `panic in an event handler`                                    | An event handler panicked                                                         |
| `panic in an action`, `panic in a rename filter`               | An action's handler or a rename filter panicked                                   |
| `panic in an inventory callback`, `panic in ready`             | An inventory callback or `Mod::ready` panicked                                    |
| `the server does not run it; restart the game to use it again` | On a client: the joined server does not run this join mod                         |
| any other text                                                 | The mod reported a fault with `Host::fault`                                       |

A mod turned off has its hooks unlinked, so the game runs vanilla where it
acted. It stays off until the game restarts.

## The game stops before the world loads

A mod that keeps a world the vanilla game cannot open safely sets a hold:
`mod-data/<id>/hold.txt`, holding the reason. While the file exists and that
mod does not run (removed, skipped, off, or the build is unknown), the loader
logs

```text
<id> holds this game (<reason>) and the mod does not run: the game stops before the world loads; update Enshwick and the mod, or delete mod-data/<id>/hold.txt to open the world anyway, at your own risk
```

and ends the process with exit code 3. On an unknown build the middle part
reads `the game build is unknown, so it would run vanilla`. Update Enshwick
and the mod, or delete `hold.txt` and accept the risk the reason names. See
[State and data folders](state.md).

## Another mod ships its own powrprof.dll

Some mods come with their own `powrprof.dll`, `dinput8.dll`, `winmm.dll` or
`dbghelp.dll`. When one of them replaced Enshwick's `powrprof.dll`, Enshwick
does not start. Put Enshwick's `powrprof.dll` back into the game's folder and
move the other mod's file into `mods/chain/`. Enshwick loads every DLL there,
in file name order, before its own loader, so both run.

## Nothing happens under Proton

The game must load Enshwick's `powrprof.dll` instead of the system one. Add
this to the game's launch options in Steam:

```text
WINEDLLOVERRIDES="powrprof=n,b" %command%
```

## The join was refused

The client compares its join mods with the server's before it joins. When the
server runs join mods the client lacks or has in another version, the client
stays out and goes back to the menu, and one dialog, "Mods missing", lists
them by where to get them:

```text
From github.com/enshwick/enshwick:
Trading Post 0.2.0, yours 0.1.0
Ask the server's admin for:
Survivors per altar 0.1.4
```

A mod's place is the homepage the server advertises for it, or the server's
admin when it advertises none. A mod you turned off in
`mods/enshwick/mods.toml` is listed under "Turn on in": turn it on and restart
the game. `loader.log` then has
`join refused: the server runs mods this client lacks`. Other cases do not keep
the player out:

- the client runs join mods the server does not: they are turned off for the
  session, with a note;
- the mods match but the loaders differ: a note asks to install the server's
  version;
- the server does not answer the query: the client joins without a check
  (`joining <address>: no rules answer`).

Only mods with `join: true` count; see [The join check](concepts/join-check.md).

## How do I remove a mod or Enshwick?

- **A mod:** delete its DLL from `mods/`. Its settings
  (`mod-configs/<id>.toml`) and data (`mod-data/<id>/`) stay; delete them too
  for a clean removal. If the mod set a hold, the game stops until you delete
  `mod-data/<id>/hold.txt` as well.
- **An update:** an update moved into place keeps the file it replaced as
  `<file>.previous`. Rename it back to roll back once.
- **Enshwick:** delete `powrprof.dll` from the game's folder. Without it
  nothing loads and the game runs vanilla; `mods/`, `mod-configs/`,
  `mod-data/` and `logs/` can then be deleted.
