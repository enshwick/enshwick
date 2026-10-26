# Furniture comfort

Every piece of furniture gives more comfort inside a base, twice vanilla by
default, so a modest house reaches a long rested bonus. Only the best piece of
each comfort category counts, as in vanilla.

## Settings

`mod-configs/furniture-comfort.toml`, written on first start; each
world keeps its own copy of it (see [Settings](../../README.md#settings)):

```toml
multiplier = 2.0
```

`multiplier` goes from 0 to 100; 1 is vanilla, and 0 takes all comfort from
furniture. Each piece's comfort is rounded to a whole number. A change applies
at once, without a restart.

## Install

Download [`enshwick-furniture-comfort.zip`](https://github.com/enshwick/enshwick/releases/download/dev/enshwick-furniture-comfort.zip)
and extract it into the folder that holds `enshrouded.exe` or
`enshrouded_server.exe`. It needs Enshwick Loader:
[`enshwick-loader.zip`](https://github.com/enshwick/enshwick/releases/download/dev/enshwick-loader.zip)
alone, or [`enshwick.zip`](https://github.com/enshwick/enshwick/releases/download/dev/enshwick.zip)
with every mod. These are development builds of the newest commit: until
the first release, 1.0.0, there are no others.

## Who needs it

Only the server, the dedicated one or a game a player hosts. Players without
the mod see the higher comfort and rested time; item tooltips keep their
vanilla numbers. It does not count in the join check.

Licensed under [MIT](../../LICENSE).
