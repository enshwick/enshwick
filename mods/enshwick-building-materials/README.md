# Building materials x2

The vanilla recipes for building blocks, roofs, overgrowth, decor and furniture
make twice as much for the same cost by default. Functional props (stations,
chests, storage shelves, altars, water devices) stay as they are. The crafting
menu shows the new counts.

This mod replaces Building yield and its Bulk Workbench. The loader never starts
an old `mods/enshwick-building-yield.dll` beside it and logs that it can be
deleted; the updater deletes it when it installs or updates this mod. A
`mod-configs/building-yield.toml` is carried over to
`mod-configs/building-materials.toml` on the first start, without the
`own_workbench` key, when the new file does not exist yet. Placed Bulk
Workbenches load as plain workbenches; Bulk Workbench items in inventories are
lost.

## Settings

`mod-configs/building-materials.toml`, written on first start; each
world keeps its own copy of it (see [Settings](../../README.md#settings)):

```toml
multiplier = 2
blocks = true
overgrowth = true
decor = true
```

- `multiplier`: 2 to 100, the output of each recipe for the same cost.
- `blocks`: building blocks and roofs.
- `overgrowth`: overgrowth materials.
- `decor`: decor and furniture.

The server's values are the ones in force: a player who joins sees the
server's counts in the crafting menu, and the file's own again after leaving,
for each server joined in a session. A change applies while the
game runs too; when the game cannot rebuild its recipes, the log says so and
the new counts wait for the next start.

## Install

Download [`enshwick-building-materials.zip`](https://github.com/themaiby/enshwick/releases/download/dev/enshwick-building-materials.zip)
and extract it into the folder that holds `enshrouded.exe` or
`enshrouded_server.exe`. It needs Enshwick Loader:
[`enshwick-loader.zip`](https://github.com/themaiby/enshwick/releases/download/dev/enshwick-loader.zip)
alone, or [`enshwick.zip`](https://github.com/themaiby/enshwick/releases/download/dev/enshwick.zip)
with every mod. These are development builds of the newest commit: until
the first release, 1.0.0, there are no others.

## Who needs it

The server and every player: the recipes change on each side, and the mod
counts in the join check.

Licensed under [MIT](../../LICENSE).
