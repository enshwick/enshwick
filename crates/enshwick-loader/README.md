# enshwick-loader

The Enshwick loader, installed as `mods/enshwick/enshwick.dll` next to the game.
The `powrprof.dll` stub starts it; it identifies the game build, loads every mod
DLL in `mods/`, resolves their dependencies, initialises them, arms their hook
chains and answers their calls. On a game build it does not know, the game runs
vanilla.

Its log is `mods/enshwick.log`.

Licensed under [MIT](../../LICENSE).
