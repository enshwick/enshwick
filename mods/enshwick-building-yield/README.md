# Building yield

A second recipe for building blocks, roofs, overgrowth, decor and furniture:
twice the output for the same cost by default. Vanilla recipes stay as they
are, and functional props (stations, chests, altars, water devices) get none.

The bulk recipes are made at the Bulk Workbench, a workbench of the mod's own,
crafted like the vanilla workbench. Each bulk recipe unlocks with its vanilla
one, wherever that one is made (a station or a survivor). With
`own_workbench = false` they sit beside the vanilla recipes at each vanilla
workshop instead; the game takes at most 512 recipes at one workshop, so where
one would overflow, some decor recipes are left out and the log says how many.

Removing the mod keeps placed Bulk Workbenches as plain workbenches; Bulk
Workbench items in inventories are lost.

## Settings

`mod-configs/building-yield.toml`, written on first start:

```toml
multiplier = 2
blocks = true
overgrowth = true
decor = true
own_workbench = true
```

## Who needs it

The server and every player: the recipes come from the mod on each side, and
the mod counts in the join check.

Licensed under [MIT](../../LICENSE).
