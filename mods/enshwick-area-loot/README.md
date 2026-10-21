# Area loot

Empty one enemy corpse and the loot of the others around it comes along. When
a player takes the last item from a corpse or an enemy loot bag, the other
enemy loot within 12 m (and 4 m up or down) goes into the same player, nearest
first, with the usual pickup messages. The emptied corpses dissolve as they do
in vanilla.

- When the inventory fills up, the mod stops at that container: what does not
  fit stays in it, and the containers further away stay untouched.
- A container another player has open, or opened in the last 10 seconds, is
  left to them.
- Chests, quest containers, items a player dropped and items on the ground keep
  the vanilla behaviour.

## Settings

`mod-configs/area-loot.toml`, written on first start:

```toml
radius = 12.0
height = 4.0
enabled = true
```

`radius` is measured across the ground from the emptied container, `height` up
and down from it, both in metres: `radius` up to 50, `height` up to 20.
Switched back on, the mod starts watching afresh, so nothing emptied while it
was off is taken. An edit takes effect about a second after saving the file,
without a restart.

## Install

Download [`enshwick-area-loot.zip`](https://github.com/themaiby/enshwick/releases/latest/download/enshwick-area-loot.zip),
always the newest version of this mod, and extract it into the folder that
holds `enshrouded.exe` or `enshrouded_server.exe`. It needs Enshwick Loader:
[`enshwick-loader.zip`](https://github.com/themaiby/enshwick/releases/latest/download/enshwick-loader.zip)
alone, or [`enshwick.zip`](https://github.com/themaiby/enshwick/releases/latest/download/enshwick.zip)
with every mod.

## Who needs it

Only the server, or the player who hosts the game. Players without the mod get
the loot and see the corpses dissolve. It does not count in the join check.

Licensed under [MIT](../../LICENSE).
