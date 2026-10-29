# enshwick-loader-stub

Internal, not for mods: a part of [Enshwick](../../../README.md). Mods
depend on [`enshwick`](../../enshwick) only.

The `powrprof.dll` that players put next to `enshrouded.exe` or
`enshrouded_server.exe`. It forwards every export to the system copy and starts
the Enshwick loader from `mods/enshwick/enshwick.dll`; without the loader the
game runs vanilla. Before the loader it loads every DLL in `mods/chain/`, in
file name order: other mods' own proxies, such as their `powrprof.dll`,
`dinput8.dll`, `winmm.dll` or `dbghelp.dll`, moved there so that both run.

Licensed under [MIT](../../../LICENSE).
