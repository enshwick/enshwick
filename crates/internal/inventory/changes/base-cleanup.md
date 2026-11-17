### Fixed

- `Effects::teleport_player_to` answers `WorldError::Unavailable` instead of
  `WorldError::Gone` on a game build that does not reflect
  `CurrentTransform`, when both players are connected.
