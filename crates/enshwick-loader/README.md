# enshwick-loader

The Enshwick loader, installed as `mods/enshwick/enshwick.dll` next to the game.
The `powrprof.dll` stub starts it; it identifies the game build, loads every mod
DLL in `mods/`, resolves their dependencies, initialises them, arms their hook
chains and answers their calls. On a game build it does not know, the game runs
vanilla.

Its log is `logs/enshwick/` (`all.log`, `loader.log`, `mods/<id>.log`). When
the game faults in a mod's code, or in game code with a mod's code on the stack,
`crash.log` there names the mod, before the game's own crash handling. Its
settings, written on first start, are `mod-configs/enshwick.toml`: `log_level`
keeps lines up to `error`, `warn`, `info` (the default) or `debug`, and
`log_levels` sets it per mod id, `enshwick` being the loader itself. The loader
watches `mod-configs/` and tells a mod when its file changes; its own file
applies at once.

Licensed under [MIT](../../LICENSE).
