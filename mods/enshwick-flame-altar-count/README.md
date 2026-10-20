# Flame altar count

Sets how many Flame Altars a world allows at each Flame level. The default
doubles the vanilla count, up to 16.

| Flame level | 0 | 1 | 2 | 3  | 4 and above |
| ----------- | - | - | - | -- | ----------- |
| Vanilla     | 1 | 2 | 4 | 6  | 8 to 16     |
| Default     | 2 | 4 | 8 | 12 | 16          |

## Settings

`mod-configs/flame-altar-count.toml`, written on first start:

```toml
altars_per_flame_level = [2, 4, 8, 12, 16, 16, 16, 16, 16, 16, 16, 16]
```

Levels past the end of the list use its last entry. Values above 16 are lowered
to 16, since the game groups only 16 altar zones into bases. An edited file
applies while the game runs.

## Who needs it

The server and every player: the count shows in the game's interface and decides
where an altar may be placed. It counts in the join check, so a player with a
different version cannot join. The table is a join value: a player who joins
runs with the server's table, whatever their own file says.

Licensed under [MIT](../../LICENSE).
