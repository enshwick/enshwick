# enshwick-loader-start

Runtime of [Enshwick](../../../README.md); not a dependency of a mod, which
depends on [`enshwick`](../../enshwick) only.

The start contract between the `powrprof.dll` stub and the loader: the
contract version and the signature of `enshwick_start`. The stub calls the
export through this type, and the loader is checked against it when it
compiles.

Licensed under [MIT](../../../LICENSE).
