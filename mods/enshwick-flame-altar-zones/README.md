# Flame altar zones

For friends who build next to each other. When the zones of two or more Flame
Altars touch or overlap, they form a cluster:

- every altar of the cluster grows by the same amount on every side, until the
  cluster covers as much ground as its altars would apart, so overlap loses
  nothing;
- no zone ever shrinks, when an altar is placed or a Flame level rises;
- every altar of a cluster takes the height of the tallest one;
- a piece may be placed across the seam between two zones of a cluster, which
  vanilla refuses.

Altars that stand apart keep their vanilla zones.

## Settings

`mod-configs/flame-altar-zones.toml`, written on first start:

```toml
clusters = true
building_across_seams = true
```

## Who needs it

The server solves the clusters, and every player needs the mod for building
across seams, which the game checks on both sides. It counts in the join check.

Licensed under [MIT](../../LICENSE).
