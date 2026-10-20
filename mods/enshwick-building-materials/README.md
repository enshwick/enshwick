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

A change applies at the next start of the game: the recipes are built once
per run. Give the server and every player the same values.

## Who needs it

The server and every player: the recipes change on each side, and the mod
counts in the join check.

Licensed under [MIT](../../LICENSE).
