# enshwick-loader

Internal, not for mods: a part of [Enshwick](../../../README.md). Mods
depend on [`enshwick`](../../enshwick) only.

The Enshwick loader, installed as `mods/enshwick/enshwick.dll` next to the game.
The `powrprof.dll` stub starts it; it identifies the game build, loads every mod
DLL in `mods/`, resolves their dependencies, initialises them, arms their hook
chains and answers their calls. On a game build without a build table it finds
the table's entries by their code patterns, and only the features and mods
that need a missing one turn off; `build_patterns = false` keeps such a build
vanilla.

Its logs are in `logs/`, one file per source and run:
`logs/enshwick_<date>_<time>.log` for the loader and
`logs/<mod id>_<date>_<time>.log` for each mod, the newest 10 runs of each
kept. When the game faults in a mod's code, or in game code with a mod's code
on the stack, the loader's file and `logs/enshwick-crash_<date>_<time>.log`
name the mod, before the game's own crash handling. Its settings, written on
first start with every key commented out, are `enshwick.toml` in the game's
folder: `log_level` keeps lines up to `error`, `warn`, `info` (the default)
or `debug`, `log_levels` sets it per mod id, `enshwick` being the loader
itself, and `diagnostics`, `diagnostics_timing`, `build_patterns` and
`release_check` are read at start. The log levels apply at once when the file
changes. With `release_check` on (the default), the loader asks GitHub once
per start, on a thread of its own, whether a newer release exists; the Mods
screen shows it as one line with the release page's address, and a dedicated
server only logs it.

`enshwick status` on the server's console writes the status summary into the
loader's log: the loader features this game build lacks table entries for,
the mods and whether they run, the mods turned off and why, what mods serve
and patch, and with `diagnostics = true` the calls of every hooked function
(`diagnostics_timing = true` times one call in 256 too).

Licensed under [MIT](../../../LICENSE).
