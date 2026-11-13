### Added

- `abi`: `HostApi::config_set`, `server_stop`, `fog_uncover` and
  `content_name`, ABI 11.26 (#1068).
- Loader: a mod writes a value of its own world settings, a secret too,
  through the console's checks; the log never shows a secret's value
  (#1068).
- Loader: a mod stops a dedicated server through the game's own Ctrl+C
  path, found by the engine route `server.console_handler` on every kept
  server build (#1068).
- Loader: the server sends a circle of a player's fog of war to uncover to
  the loader on the player's client, which hands it to the game's reveal
  (#1068).
- Loader: the item picker of an action's item argument lists the items
  mods serve, by the names their texts give (#1068).
- `test`: `FakeHost::declared_content`, `Installed::settings_text`,
  `Installed::stop_asked` and `Installed::fog_uncovers` (#1068).

### Fixed

- An item argument's GUID is checked exactly for a mod's item too: a GUID
  that only shares a mod item's game id is refused before the handler runs
  (#1068).
