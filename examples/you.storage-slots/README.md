# Storage Slots

A data pack for Enshrouded, run by the [Enshwick](https://github.com/enshwick/enshwick) loader.
It changes the slots of every storage box in `items.toml`:

- the smallest box gets one more row of 8 slots;
- each bigger box gets more, up to five more rows for the largest.

Every box gets the new size when a world loads, boxes placed before the pack
too. Removing the pack loses no item: a box that held more slots than the
game gives it comes back with 128 slots, every item in its slot.

1. Run `cargo enshwick pack check . --game <game folder>` to check every file
   against the game's data, with the line of each problem.
2. Copy this folder into the game's `mods/` folder as `mods/you.storage-slots/`, on the
   server and on every player's game.
3. Run `cargo enshwick pack zip .` for the archive players install.
