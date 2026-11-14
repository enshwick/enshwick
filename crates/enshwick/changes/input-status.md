### Fixed

- `Input::local_held` returns the local player's held controls in a world
  again: the loader read them before the game's interface system took its
  player, so every read answered `None` (#1109).
- A game build whose interface system lays out its player query
  differently turns the held controls off with a log line instead of
  reading the wrong memory (#1109).
- The fake host's `local_held` answers `None` at a mod's first ask, as the
  loader does, which starts its reads there (#1109).
