### Added

- `Load::on_every_client_raw_game_event` (#1081).

### Changed

- `Load::on_client_raw_game_event` needs the `unstable` feature and never
  runs its handler on a dedicated server, where it answers without an error
  (#1081).
