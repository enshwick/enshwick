# Building materials x2

Будматеріали ×2.

The vanilla recipes for building blocks, roofs, overgrowth, decor and
furniture make twice as much for the same cost by default. Functional props
(stations, chests, altars, water devices) stay as they are. The crafting menu
shows the new counts.

This mod replaces Building yield and its Bulk Workbench: delete
`mods/enshwick-building-yield.dll` from the server and every client. A
`mod-configs/building-yield.toml` is carried over to
`mod-configs/building-materials.toml` on the first start, without the
`own_workbench` key. Placed Bulk Workbenches load as plain workbenches; Bulk
Workbench items in inventories are lost.

## Settings

`mod-configs/building-materials.toml`, written on first start:

```toml
multiplier = 2    # 2 to 100
blocks = true     # building blocks and roofs
overgrowth = true # overgrowth materials
decor = true      # decor and furniture
```

The server's values are the ones in force: a player who joins sees the
server's counts in the crafting menu, and the file's own again after leaving,
for each server joined in a session. An edit of the file applies while the
game runs too.

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
