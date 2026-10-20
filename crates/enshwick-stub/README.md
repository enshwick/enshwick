# enshwick-stub

The `powrprof.dll` that players put next to `enshrouded.exe` or
`enshrouded_server.exe`. It forwards every export to the system copy and starts
the Enshwick loader from `mods/enshwick/enshwick.dll`; without the loader the
game runs vanilla.

Before it starts the loader, it applies staged updates: every file an updater
left under `mods/.staged/` moves to the same path under the game's folder, and
the file it replaces stays beside it as `<file>.previous` for one rollback. A
file that cannot be moved stays staged for the next start, and the game starts
either way. What it did goes to `logs/enshwick/stub.log`.

Licensed under [MIT](../../LICENSE).
