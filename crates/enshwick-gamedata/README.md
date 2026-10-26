# enshwick-gamedata

Internal, not for mods: a part of [Enshwick Loader](https://github.com/enshwick/enshwick). Mods
depend on [`enshwick`](https://crates.io/crates/enshwick) only.

The loader's build tables: per game build, the engine functions and constants
reflection does not describe, by symbolic id, as SKSE's Address Library does.
Each table is a TOML file in [`tables/`](https://github.com/enshwick/enshwick/tree/main/crates/enshwick-gamedata/tables), named
`<side>-<guid>-<age>.toml` after the build's fingerprint. A table also
records the build's Steam build id (`steam_build`) and the game's own
version number (`game_version`), each where it is known; several Steam
builds can share one game version.

The catalogue (`catalogue.rs`) maps the ids the API asks engine facts by,
such as `item_info.name`, to the game's reflected types and fields, so a
game update that moves a field changes this crate, not the mods.

- The loader embeds every table of this folder, and reads no other: a table
  for a newer game build ships with the next loader build.
- On a build without a table, the loader takes the newest table of the same
  side, by Steam build id, and finds each entry by its `pattern`: a symbol at or through the
  code it matches, a constant read from the operand of the instruction that
  uses it, or in the code of a symbol it names (`via`), at its start or
  once `within` a number of bytes from it. The slots of
  attributes name their `attribute` and come from the game's archive, which
  the loader reads on every build. A value only its own build holds, such
  as a slot in a function table, says `carried = false` and is left out.
  What neither finds stays missing, and only the features and mods that
  need it turn off.
- [`cargo enshwick pin`](https://github.com/enshwick/enshwick/blob/main/crates/cargo-enshwick/README.md#porting-build-rows-pin)
  writes the table for a new build, patterns included.
