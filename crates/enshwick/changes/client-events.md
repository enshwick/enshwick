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

- Loader: while a hosted game starts, and after it ends until its server is
  torn down, the events of its server's world no longer reach the client's
  handlers (#1081).
- Loader: the client's notices are raised before its world's events move,
  also while client game events are off, and never reach the client's
  handlers as game events (#1081).
- Loader: on a build without the event stream layout, a client lists client
  game events as off when a mod subscribed to them (#1081).
- A client game event subscription on a dedicated server answers without an
  error and is not kept, so a `both` mod no longer turns off there (#1081).
- The loader and the fake host refuse a game event subscription outside the
  mod's init (#1081).
