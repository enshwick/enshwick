# enshwick-loader

Internal, not for mods: a part of [Enshwick Loader](../../../README.md). Mods
depend on [`enshwick`](../../enshwick) only.

The Enshwick loader, installed as `mods/enshwick/enshwick.dll` next to the game.
The `powrprof.dll` stub starts it; it identifies the game build, loads every mod
DLL in `mods/`, resolves their dependencies, initialises them, arms their hook
chains and answers their calls. On a game build without a build table it finds
the table's entries by their code patterns, and only the features and mods
that need a missing one turn off; `build_patterns = false` keeps such a build
vanilla.

Its log is `logs/enshwick/` (`all.log`, `loader.log`, `mods/<id>.log`). When
the game faults in a mod's code, or in game code with a mod's code on the stack,
`crash.log` there names the mod, before the game's own crash handling. Its
settings, written on first start, are `mod-configs/enshwick.toml`: `log_level`
keeps lines up to `error`, `warn`, `info` (the default) or `debug`, and
`log_levels` sets it per mod id, `enshwick` being the loader itself. The loader
watches `mod-configs/` and tells a mod when its file changes; its own file
applies at once.

`logs/enshwick/status.log` holds the latest status summary: the mods and
whether they run, the mods turned off and why, what mods serve and patch, and
with `diagnostics = true` the calls of every hooked function
(`diagnostics_timing = true` times one call in 256 too). It is written every
`status_minutes` (10 by default) and for `enshwick status` in the
[admin tools'](https://github.com/enshwick/mod-admin-tools) command file.

Licensed under [MIT](../../../LICENSE).
