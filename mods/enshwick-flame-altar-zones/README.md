# Flame altar zones

For friends who build next to each other. When the zones of two or more Flame
Altars touch or overlap, they form a cluster:

- every altar of the cluster grows by the same amount on every side, until the
  cluster covers as much ground as its altars would apart, so overlap loses
  nothing;
- no zone ever shrinks, when an altar is placed or a Flame level rises, nor
  while the server stalls for a moment;
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

`building_across_seams` is the server's: a joined player runs with the
server's value, and a change applies while the game runs. `clusters` applies
after a restart.

## Install

Download [`enshwick-flame-altar-zones.zip`](https://github.com/themaiby/enshwick/releases/download/dev/enshwick-flame-altar-zones.zip)
and extract it into the folder that holds `enshrouded.exe` or
`enshrouded_server.exe`. It needs Enshwick Loader:
[`enshwick-loader.zip`](https://github.com/themaiby/enshwick/releases/download/dev/enshwick-loader.zip)
alone, or [`enshwick.zip`](https://github.com/themaiby/enshwick/releases/download/dev/enshwick.zip)
with every mod. These are development builds of the newest commit: until
the first release, 1.0.0, there are no others.

## Who needs it

The server solves the clusters, and every player needs the mod for building
across seams, which the game checks on both sides. It counts in the join check.

Licensed under [MIT](../../LICENSE).
