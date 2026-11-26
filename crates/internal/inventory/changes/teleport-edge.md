### Fixed

- `Effects::teleport`, `teleport_player` and `teleport_player_to` answer
  `WorldError::OutsideWorld` for a target past the box the world's scene
  keeps players in, and `WorldError::NotLoaded` in the first moments of a
  game, while the loader reads that box (#1299).
