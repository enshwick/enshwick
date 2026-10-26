# enshwick-loader-stub

Internal, not for mods: a part of [Enshwick Loader](../../../README.md). Mods
depend on [`enshwick`](../../enshwick) only.

The `powrprof.dll` that players put next to `enshrouded.exe` or
`enshrouded_server.exe`. It forwards every export to the system copy and starts
the Enshwick loader from `mods/enshwick/enshwick.dll`; without the loader the
game runs vanilla.

Licensed under [MIT](../../../LICENSE).
