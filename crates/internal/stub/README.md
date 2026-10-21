# enshwick-loader-stub

Internal, not for mods: a part of [Enshwick Loader](../../../README.md). Mods
depend on [`enshwick-loader-api`](../../enshwick-loader-api) only.

The `powrprof.dll` that players put next to `enshrouded.exe` or
`enshrouded_server.exe`. It forwards every export to the system copy and starts
the Enshwick loader from `mods/enshwick/enshwick.dll`; without the loader the
game runs vanilla.

Before it starts the loader, it applies staged updates: each update an
updater left complete as a folder `mods/.staged/<digits>/`, oldest first, has
every file in it moved to the same path under the game's folder, and the file
it replaces stays beside it as `<file>.previous` for one rollback. Anything
else in `mods/.staged/`, such as an update a crash cut short, stays alone. A
file that cannot be moved stays staged for the next start, and the game starts
either way. What it did goes to `logs/enshwick/stub.log`.

Licensed under [MIT](../../../LICENSE).
