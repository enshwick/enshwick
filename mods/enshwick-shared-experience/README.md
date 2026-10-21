# Shared experience

Friends who play at different hours stay on the same character level. Each
world remembers the most experience any character has reached in it, and
every character below it catches up: while online, and on the next join for
those who were away. A new world starts without a mark.

- Every character's play moves the mark: what anyone earns raises it, so a
  character catching up still advances everyone.
- Playing together does not speed anyone up. When several players earn
  experience in the same moment, as from one kill, only the largest gain of
  that moment counts, once.
- A character behind the leader gains the difference, one level per tick, with
  the level-up banner, skill points and saved level as if earned in play.
- Characters already ahead, including those brought from another world, raise
  the mark for everyone.
- Nothing grows past the level cap. Flame level is shared in vanilla already;
  skill points from knowledge stay per character.

Experience granted here is part of the character and travels with it to other
worlds.

## Settings

`mod-configs/shared-experience.toml`, written on first start; each
world keeps its own copy of it (see [Settings](../../README.md#settings)):

```toml
enabled = true
```

A change applies at once, without a restart.

## Install

Download [`enshwick-shared-experience.zip`](https://github.com/themaiby/enshwick/releases/download/dev/enshwick-shared-experience.zip)
and extract it into the folder that holds `enshrouded.exe` or
`enshrouded_server.exe`. It needs Enshwick Loader:
[`enshwick-loader.zip`](https://github.com/themaiby/enshwick/releases/download/dev/enshwick-loader.zip)
alone, or [`enshwick.zip`](https://github.com/themaiby/enshwick/releases/download/dev/enshwick.zip)
with every mod. These are development builds of the newest commit: until
the first release, 1.0.0, there are no others.

## Who needs it

Only the server, or the player who hosts the game. Players without the mod
see their bar fill and their level rise. It does not count in the join check.
The mark of each world is kept in
`mod-data/shared-experience/worlds/<world>/experience.toml` across restarts,
on a dedicated server and in a hosted game alike. A mark kept for the whole
server by an older version, `mod-data/shared-experience/experience.toml`,
moves once into the first world that loads without a mark of its own and is
renamed `experience.toml.moved`.

Licensed under [MIT](../../LICENSE).
