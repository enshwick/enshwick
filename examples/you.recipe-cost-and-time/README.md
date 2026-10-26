# Recipe cost and time

A data pack for Enshrouded, run by the [Enshwick](https://github.com/enshwick/enshwick) loader.
It changes three production recipes in `recipes.toml`:

- bronze bars at the smelter, fired bricks at the kiln and flat bread at the
  oven take half their ingredients;
- each of them takes half its crafting time.

The game's other recipes stay as they are.

1. Run `cargo enshwick pack check . --game <game folder>` to check every file
   against the game's data, with the line of each problem.
2. Copy this folder into the game's `mods/` folder as `mods/you.recipe-cost-and-time/`, on the
   server and on every player's game.
3. Run `cargo enshwick pack zip .` for the archive players install.
