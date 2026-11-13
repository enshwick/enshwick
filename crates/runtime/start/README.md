# enshwick-loader-start

Runtime of [Enshwick](../../../README.md); not a dependency of a mod, which
depends on [`enshwick`](../../enshwick) only.

The start contract between the `powrprof.dll` stub and the loader: the name
of the loader's start export, which carries the contract's version, and its
type. The stub calls the export through this type, and the loader is checked
against it when it compiles. A stub and a loader of different contracts find
no common export, so the game runs without mods.

Licensed under [MIT](../../../LICENSE).
