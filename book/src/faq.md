# FAQ and troubleshooting

## Where are the logs?

Everything is under `logs/enshwick/` in the game's folder (beside
`enshrouded.exe` or `enshrouded_server.exe`):

| File            | What it holds                                        |
| --------------- | ---------------------------------------------------- |
| `all.log`       | Every line, from the loader and every mod            |
| `loader.log`    | The loader's lines                                   |
| `mods/<id>.log` | One mod's lines                                      |
| `status.log`    | The latest status summary                            |
| `crash.log`     | Faults the loader attributed to a mod or to itself   |
| `stub.log`      | What the `powrprof.dll` stub did with staged updates |

`all.log`, `loader.log` and the mod logs start empty each session. Each line
names its time (UTC), level and source:

```text
01:02:03.004Z warn  [flame-altar-zones] merged 3 altars
```

How much is kept is set in `mod-configs/enshwick.toml`, for all sources and
per mod id (`enshwick` is the loader); see
[Logging and diagnostics](logging.md):

```toml
log_level = "info"
log_levels = { flame-altar-zones = "debug" }
```

## What is in crash.log?

The loader watches for hardware faults: access violations, illegal or
privileged instructions, integer division by zero and stack overflows. When
the faulting address is in a mod's DLL or the loader's, or (except for a stack
overflow) a return address into one is near the top of the stack, it appends
one line to `crash.log` and writes the same line to `loader.log`:

```text
fault 0xc0000005 at 0x7ffb1a2b3c4d (+0x3c4d in safes), in its code; access 0x0
```

The parts are the exception code, the address, the offset in the module and
whose module it is, and `in its code` or
`in game code called with its code on the stack`. The last value is the
address the faulting access touched. The loader records at most five faults a
session and does not handle them: the game's own crash handling runs as it
would without Enshwick. A fault with no mod or loader code involved is not
recorded.

`crash.log` is appended to across sessions, unlike the other logs.

## What is in status.log?

A summary the loader writes a minute after start, then every
`status_minutes` (10 by default, 0 for on demand only), and when an
administrator types `/enshwick status`. Each write replaces the last. It lists
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

| Message in `loader.log`                                                             | What it means                                                    |
| ----------------------------------------------------------------------------------- | ---------------------------------------------------------------- |
| `unknown game build <guid>/<age>; the game runs vanilla until Enshwick supports it` | The game was updated; Enshwick needs a release for the new build |
| `<name> is not where the build table says; nothing loads`                           | The build looks known but does not match; treat it as unknown    |
| `this is not Enshrouded or its dedicated server; nothing loads`                     | `powrprof.dll` was loaded by another program                     |

On an unknown build every mod's hold applies; see
[The game stops before the world loads](#the-game-stops-before-the-world-loads).

### The mod was skipped

These lines read `<file> skipped: <reason>`, or name the file alone:

| Message                                               | What it means                                                         |
| ----------------------------------------------------- | --------------------------------------------------------------------- |
| `<file> is not an Enshwick mod`                       | The DLL lacks the four exports of `declare_mod!`                      |
| `<file> could not be loaded`                          | Windows could not load the DLL, such as a missing dependency          |
| `<file> is built for ABI 2, the loader speaks 1`      | The mod was built for another ABI major                               |
| `skipped: not for this side of the game`              | A client-only mod on the dedicated server                             |
| `skipped: a second copy of safes, after #0`           | Two DLLs with the same id; the first one loads                        |
| `skipped: requires trading-post, which is not loaded` | A required mod is absent or was skipped itself                        |
| `skipped: requires trading-post ^0.2, found 0.1.3`    | A required mod is too old or too new                                  |
| `skipped: conflicts with other-mod`                   | Of two conflicting mods, the later one gives way                      |
| `skipped: in a dependency cycle`                      | Mods require each other in a circle                                   |
| `skipped: the id "My Mod" is not [a-z0-9-]+`          | The manifest's id is invalid; other manifest errors read the same way |

The client loads server mods too, since it runs the server's code while it
hosts a game; see [Sides and hosting](concepts/sides-and-hosting.md).

### The mod turned itself off in init

The mod's log has `off: <reason>` at `warn`, then the loader's
`off: init returned 3`. Common reasons, as the safe layer words them:

| Reason                                                  | What it means                                                          |
| ------------------------------------------------------- | ---------------------------------------------------------------------- |
| `symbol quest.reward: missing on this game build`       | Neither the mod nor the loader pins the symbol for this build          |
| `constant ecs.context_row: missing on this game build`  | The same for an engine constant                                        |
| `settings: <parse error>`                               | `mod-configs/<id>.toml` does not parse into the settings type          |
| `hook REWARD: refused by the loader`                    | Another mod holds the function exclusively, or the mod hooked it twice |
| `service <name> v<version>: missing on this game build` | The publishing mod is absent, off or loads later                       |
| `...: needs a newer loader`                             | The loader is older than the ABI minor the feature needs               |

See [ABI reference](abi-reference.md#under-an-older-loader) for the features
each minor added. A settings file that stops parsing while the game runs does
not turn the mod off: the mod logs
`settings change skipped, the running values stay: ...` and keeps its values.

### The mod was turned off later

| `off:` reason                                                  | What happened                                                                     |
| -------------------------------------------------------------- | --------------------------------------------------------------------------------- |
| `init returned 6` after `off: init panicked`                   | `Mod::init` panicked                                                              |
| `its hook at 0x... could not be armed`                         | The loader could not detour the function; `hook at 0x... not armed: ...` says why |
| `its code patch at 0x... was skipped`                          | The bytes no longer read as expected, or overlap a hook                           |
| `panic in hook REWARD`                                         | A detour panicked; the call went on to the original                               |
| `panic in the hook at 0x...`                                   | A mid-function hook's handler panicked                                            |
| `panic in an event handler`                                    | An event handler panicked                                                         |
| `panic in a command handler`, `panic in a rename filter`       | A chat command handler or rename filter panicked                                  |
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

## Nothing happens under Proton

The game must load Enshwick's `powrprof.dll` instead of the system one. Add
this to the game's launch options in Steam:

```text
WINEDLLOVERRIDES="powrprof=n,b" %command%
```

## The join was refused

The client compares its join mods with the server's before it joins. When the
server runs join mods the client lacks or has in another version, a note
lists them and the client stays out:

```text
This server runs Enshwick mods you do not have in its version:

safes: server 0.2.0, yours 0.1.0
survivors-per-altar: server 0.1.4, not installed

Install the server's versions to join.
```

`loader.log` then has `join refused: the server runs mods this client lacks`.
Install the server's versions; the [updater](releasing.md#the-updater-mod)
offers them once the note closes. Other cases do not keep the player out:

- the client runs join mods the server does not: they are turned off for the
  session, with a note;
- the mods match but the loaders differ: a note asks to install the server's
  release;
- the server does not answer the query: the client joins without a check
  (`joining <address>: no rules answer`).

Only mods with `join = true` count; see [The join check](concepts/join-check.md).

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
