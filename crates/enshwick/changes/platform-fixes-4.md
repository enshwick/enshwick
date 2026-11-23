### Added

- `test`: `Installed::set_saves_refused` refuses the world's saves from
  outside the mod, as another mod's hold or the game's own block does, so a
  test reaches the mod's answer to a refused `World::save`. The shutdown
  lets go of it (#1237).

### Fixed

- In a game hosted from a client, the host's own player reads the game's
  language in `World::language_of` and in a `BuildingPlaced` or
  `BuildingRemoved` handler, where it read the fallback before its hello
  arrived. The fake host answers the same (#1237).
