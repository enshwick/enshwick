# Build camera

Opens the building fly mode of the Airborne Architect skill to every player:
near a base, the character stays put while a flying spark carries the camera,
and blocks and props are placed and removed from there. No skill points are
needed.

Everything else stays vanilla:

- fly mode starts only near a building zone, for a player allowed to edit the
  base, alive and not gliding, and ends when one of these no longer holds;
- build reach is the vanilla one, measured from the spark;
- the skill tree still shows Airborne Architect as locked, and points spent on
  it are not refunded. Other skills and their prerequisites are untouched.

## Settings

`mod-configs/build-camera.toml`, written on first start:

```toml
enabled = true
```

`false` asks for the skill again, as in vanilla. An edited file applies while
the game runs.

## Install

Download [`enshwick-build-camera.zip`](https://github.com/themaiby/enshwick/releases/latest/download/enshwick-build-camera.zip),
always the newest version of this mod, and extract it into the folder that
holds `enshrouded.exe` or `enshrouded_server.exe`. It needs Enshwick Loader:
[`enshwick-loader.zip`](https://github.com/themaiby/enshwick/releases/latest/download/enshwick-loader.zip)
alone, or [`enshwick.zip`](https://github.com/themaiby/enshwick/releases/latest/download/enshwick.zip)
with every mod.

## Who needs it

The server and every player: the server decides fly mode, and the player's
game predicts the same decision, so both must agree. The mod counts in the
join check, and `enabled` is a join value: a player who joins runs with the
server's value, whatever their own file says.

Licensed under [MIT](../../LICENSE).
