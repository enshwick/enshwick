# enshwick-loader-builds

Internal, not for mods: a part of [Enshwick Loader](../../../README.md). Mods
depend on [`enshwick-loader-api`](../../enshwick-loader-api) only.

The loader's build tables: per game build, the engine functions and constants
reflection does not describe, by symbolic id, as SKSE's Address Library does.
Each table is a TOML file in [`tables/`](tables), named
`<side>-<guid>-<age>.toml` after the build's fingerprint.

- The loader embeds every table of this folder. A table the updater fetches
  for a newer game build lands in `mods/enshwick/builds/` with its minisign
  signature, and the loader uses it only when the project key signed it.
- On a build without a table, the loader takes the newest table of the same
  side and finds each entry by its `pattern`. What no pattern finds stays
  missing, and only the features and mods that need it turn off.
- [`cargo enshwick pin`](../../cargo-enshwick/README.md#porting-build-rows-pin)
  writes the table for a new build, patterns included.
