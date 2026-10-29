# Material Stacks

A data pack for Enshrouded, run by the [Enshwick](https://github.com/enshwick/enshwick) loader.
It changes one value in `items.toml`:

- wood logs, wood planks, stone, resin and charcoal stack to 999 in one
  inventory slot.

A world saved with these stacks depends on the pack: read
[stack sizes and saved worlds](../../book/src/content/packs.md#stack-sizes-and-saved-worlds)
before you remove it.

1. Run `cargo enshwick pack check . --game <game folder>` to check every file
   against the game's data, with the line of each problem.
2. Copy this folder into the game's `mods/` folder as `mods/you.material-stacks/`, on the
   server and on every player's game.
3. Run `cargo enshwick pack zip .` for the archive players install.
