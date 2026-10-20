# Flame altar count

Sets how many Flame Altars a world allows at each Flame level, up to 32 altars
in 16 bases. The default doubles the vanilla count.

| Flame level | 0 | 1 | 2 | 3  | 4  | 5  | 6  | 7  | 8  | 9  | 10 | 11 and above |
| ----------- | - | - | - | -- | -- | -- | -- | -- | -- | -- | -- | ------------ |
| Vanilla     | 1 | 2 | 4 | 6  | 8  | 10 | 11 | 12 | 13 | 14 | 15 | 16           |
| Default     | 2 | 4 | 8 | 12 | 16 | 20 | 22 | 24 | 26 | 28 | 30 | 32           |

## Settings

`mod-configs/flame-altar-count.toml`, written on first start:

```toml
altars_per_flame_level = [2, 4, 8, 12, 16, 20, 22, 24, 26, 28, 30, 32]
```

Levels past the end of the list use its last entry. Values above 32 are lowered
to 32. An edited file applies while the game runs.

## Altars past 16

The game itself takes 16 altars: it groups the zones of only 16 altars into
bases, and holds 16 bases. The mod changes the game's grouping, on the server
and on every player's game, so that it takes 32 altars:

- altars whose zones touch, directly or through other altars, share one base,
  as in vanilla;
- a world holds at most 16 bases: an altar that would start a 17th base, one
  whose zone touches no base, is refused as "too many altars". Place it so its
  zone touches one of your bases;
- if a 17th base comes about anyway, its zone still allows building, but it
  is no base (no comfort, crafting from chests or survivors) until it touches
  another base's zone, and the log says so.

If the mod does not find the game code it changes, such as after a game update,
it keeps the limit of 16 and says so in its log.

The save keeps every altar as vanilla does. A world with more than 16 altars
needs the mod to open safely: without it the game groups the altars past 16 on
its own and may crash. So while the world has more than 16 altars, the
dedicated server holds the game: when a game update makes the build unknown to
Enshwick, the mod is missing or off, or it cannot find its places in the game,
Enshwick stops the server before the world loads and says why in
`logs/enshwick/loader.log`. Only a dedicated server keeps the hold: a player
hosting such a world must keep the mod running and up to date. Update Enshwick and the
mod, or remove altars down to 16 with the mod running. To open the world anyway,
at your own risk, delete `mod-data/flame-altar-count/hold.txt`.

## Who needs it

The server and every player: the count shows in the game's interface, decides
where an altar may be placed, and every player's game groups the altars into
bases. It counts in the join check, so a player with a different version cannot
join. The table is a join value: a player who joins runs with the server's
table, whatever their own file says.

Licensed under [MIT](../../LICENSE).
