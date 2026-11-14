### Fixed

- `Input::local_held` returns the local player's held controls in a world
  again: the loader read them before the game's interface system took its
  player, so every read answered `None` (#1109).
