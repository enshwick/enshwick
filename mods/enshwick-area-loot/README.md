# Area loot

Empty one enemy corpse and the loot of the others around it comes along. When
a player takes the last item from a corpse or an enemy loot bag, the other
enemy loot within 12 m (and 4 m up or down) goes into the same player, nearest
first, with the usual pickup messages. The emptied corpses dissolve as they do
in vanilla.

- When the inventory fills up, the mod stops at that container: what does not
  fit stays in it, and the containers further away stay untouched.
- A container another player has open, opened in the last 10 seconds, or last
  opened and still stands within 4 m of, is left to them, also when they open it
  or walk up to it while the containers before it are being emptied: each
  container is checked again right before its turn. The seconds count only while the server runs, so
  a stalled server hands nothing to the wrong player.
- Chests, quest containers, items a player dropped and items on the ground keep
  the vanilla behaviour.

## Settings

`mod-configs/area-loot.toml`, written on first start; each
world keeps its own copy of it (see [Settings](../../README.md#settings)):

```toml
radius = 12.0
height = 4.0
enabled = true
```

`radius` is measured across the ground from the emptied container, `height` up
and down from it, both in metres: `radius` from 0.1 to 50, `height` from 0.1
to 20. A file with a value outside these is skipped, and the running values
stay.
Switched back on, the mod starts watching afresh, so nothing emptied while it
was off is taken. A change applies at once,
without a restart.

## Install

Download [`enshwick-area-loot.zip`](https://github.com/enshwick/enshwick/releases/download/dev/enshwick-area-loot.zip)
and extract it into the folder that holds `enshrouded.exe` or
`enshrouded_server.exe`. It needs Enshwick Loader:
[`enshwick-loader.zip`](https://github.com/enshwick/enshwick/releases/download/dev/enshwick-loader.zip)
alone, or [`enshwick.zip`](https://github.com/enshwick/enshwick/releases/download/dev/enshwick.zip)
with every mod. These are development builds of the newest commit: until
the first release, 1.0.0, there are no others.

## Who needs it

Only the server, or the player who hosts the game. Players without the mod get
the loot and see the corpses dissolve. It does not count in the join check.

## For mod authors

Other mods may leave out a container around the emptied one that would
come along, at the extension point `area-loot:eligible`; they never add
one the mod passed over, such as a container another player is at. See
[Extending other mods](https://github.com/enshwick/enshwick/blob/main/book/src/extending.md).

Licensed under [MIT](../../LICENSE).
