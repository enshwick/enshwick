### Added

- `FakeHost::game_settings` lays out the game settings as the current build
  does, so `Load::game_settings` resolves in a mod's test, and
  `Installed::set_default_game_settings` fills them with the dedicated
  server's Default preset (#1305).
- `FakeHost::without_saves` plays a build without the save code: a save and
  a save hold answer unavailable, as on the loader (#1305).
