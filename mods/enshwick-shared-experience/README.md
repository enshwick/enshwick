# Shared experience

Friends who play at different hours stay on the same character level. The
server remembers the most experience any character has reached, and every
character below it catches up: while online, and on the next join for those
who were away.

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

`mod-configs/shared-experience.toml`, written on first start:

```toml
enabled = true
```

An edit takes effect about a second after saving the file, without a restart.

## Who needs it

Only the server. Players without the mod see their bar fill and their level
rise. It does not count in the join check. A dedicated server keeps the mark in
`mod-data/shared-experience/experience.toml` across restarts; a game a client
hosts starts from the characters present, as its host is always one of them.

Licensed under [MIT](../../LICENSE).
