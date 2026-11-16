### Fixed

- A lethal fall that a damage filter spared no longer counts as a death:
  the game decided on the death from the damage before the filter, so it
  recorded a player death and the loader raised `events::PlayerDied` for
  a player who kept their health (#1146).
