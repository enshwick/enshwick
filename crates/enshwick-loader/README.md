# enshwick-loader

The Enshwick loader, installed as `mods/enshwick/enshwick.dll` next to the game.
The `powrprof.dll` stub starts it; it identifies the game build, loads every mod
DLL in `mods/`, resolves their dependencies, initialises them, arms their hook
chains and answers their calls. On a game build it does not know, the game runs
vanilla.

Its log is `logs/enshwick/` (`all.log`, `loader.log`, `mods/<id>.log`). Its
settings, written on first start, are `mod-configs/enshwick.toml`: `log_level`
keeps lines up to `error`, `warn`, `info` (the default) or `debug`, and
`log_levels` sets it per mod id, `enshwick` being the loader itself.

Licensed under [MIT](../../LICENSE).
