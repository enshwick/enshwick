# A mod without Rust

A data pack changes the game with TOML files in a folder, without code. Here
you make one that lets four times as many wood logs fit in one inventory slot.

## Create the pack

1. Run `cargo enshwick new --data you.bigger-stacks`. It makes the folder
   `you.bigger-stacks/` with `pack.toml`, one file per kind of data and a
   README.

   Without the tool, make the folder by hand with a `pack.toml` of two lines:

   ```toml
   id = "you.bigger-stacks"
   version = "0.1.0"
   ```

2. Open `items.toml` and write the change:

   ```toml
   [[change]]
   items = ["Material_T1_Wood_Logs"]
   max_stack = { times = 4 }
   ```

   Items go by the game's own names. A number sets the value, and
   `{ times = 4 }` multiplies it.

## Check it

1. Run `cargo enshwick pack check you.bigger-stacks --game <game folder>`. It
   reads every file against the game's data and prints each problem with its
   file and line, with the names that look like a misspelt one:

   ```text
   error: items.toml:2:1: no item "Material_T1_Wood_Log" in this pack, and no vanilla item "Material_T1_Wood_Log"; near: Material_T1_Wood_Logs
   ```

## Install and play

1. Copy the folder into the game's `mods/` folder, on the server and in every
   player's game: the join check lets only players with the same pack in.
2. Start the game. The pack's log, `logs/enshwick/mods/you.bigger-stacks.log`,
   counts what it changed:

   ```text
   info  [you.bigger-stacks] data pack: 0 ids, 0 loot drops, 1 changes
   ```

3. Pick up wood logs: a slot now holds four times as many.

A world saved with larger stacks depends on the pack from then on, and the
game stops before it loads that world without it. Read
[stack sizes and saved worlds](packs.md#stack-sizes-and-saved-worlds) before
you remove it.

## Release it

1. Run `cargo enshwick pack zip you.bigger-stacks` and attach the archive to
   your release; players unpack it into the game's folder.

## Next

[Data packs](packs.md) lists every change a pack can make: recipe costs and
times, chest slots, item stats, new items, workshops and loot. With
`dev_reload = true`, changed values apply while the game runs.
