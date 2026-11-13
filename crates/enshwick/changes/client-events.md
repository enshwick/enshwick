### Added

- `Load::on_every_client_raw_game_event`, and in Luau
  `enshwick.on_client_game_type`, for the events of the client's own world
  (#1081).

### Changed

- `Load::on_client_raw_game_event` and `EventType::on_client` need the
  `unstable` feature (#1081).
- A client game event subscription takes 0 for every type and has no cap per
  mod (#1081).

### Fixed

- Loader: while a hosted game starts, the events of its server's world no
  longer reach the client's handlers (#1081).
- Loader: the client's notices are raised before its world's events move,
  also while client game events are off (#1081).
- Loader: on a build without the event stream layout, a client lists client
  game events as off (#1081).
- A client game event subscription on a dedicated server is kept without an
  error, so a `both` mod no longer turns off there (#1081).
- The fake host refuses a game event subscription after init, as the loader
  does (#1081).
