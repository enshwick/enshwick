# Storage slots

A data pack for Enshrouded, run by the [Enshwick](https://github.com/enshwick/enshwick) loader.
It changes the slots of every storage box in `items.toml`:

- the smallest box gets one more row of 8 slots;
- each bigger box gets more, up to five more rows for the largest.

Boxes placed before the pack keep their size; boxes placed or upgraded after
it get the new one. Removing the pack leaves every box as it is.

1. Run `cargo enshwick pack check . --game <game folder>` to check every file
   against the game's data, with the line of each problem.
2. Copy this folder into the game's `mods/` folder as `mods/you.storage-slots/`, on the
   server and on every player's game.
3. Run `cargo enshwick pack zip .` for the archive players install.
