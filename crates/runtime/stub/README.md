# enshwick-loader-stub

Runtime of [Enshwick](../../../README.md), shipped in `enshwick.zip`; not a
dependency of a mod, which depends on [`enshwick`](../../enshwick) only.

The `powrprof.dll` that players put next to `enshrouded.exe` or
`enshrouded_server.exe`. It forwards every export to the system copy and starts
the Enshwick loader from `mods/enshwick/enshwick.dll`; without the loader the
game runs vanilla.

Licensed under [MIT](../../../LICENSE).
