# Building yield

A second recipe beside the vanilla one for building blocks, roofs, overgrowth,
decor and furniture: twice the output for the same cost by default. Vanilla
recipes stay as they are, and functional props (stations, chests, altars, water
devices) get none.

The game takes at most 512 recipes at one workshop, so where a workshop would
overflow, some decor recipes are left out; the log says how many. Blocks,
roofs and overgrowth come first. Once the API can add workshops, the bulk
recipes move to a bulk workbench of their own and none are left out.

## Settings

`mod-configs/building-yield.toml`, written on first start:

```toml
multiplier = 2
blocks = true
overgrowth = true
decor = true
```

## Who needs it

The server and every player: the recipes come from the mod on each side, and
the mod counts in the join check.

Licensed under [MIT](../../LICENSE).
