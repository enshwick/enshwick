### Added

- `CommandAction::Rebind` binds a key to a mod's control, or clears it, and
  `CommandAction::SetSetting` sets a mod's setting, each on that mod's page
  of the loader's Mods screen through the page's own paths, so the loader
  writes `mod-controls.toml` or the world's settings file as a click does;
  observed once the write is queued, and `CancelReason::ChangeRefused` when
  the page refuses the change.
- `CommandAction::Quit` quits the game from the main menu through the
  game's own quit command; observed once the interface's queue takes it.
- `abi::ClientCommand` gains the kinds `REBIND`, `SET_SETTING` and `QUIT`,
  the fields `key` and `key_slot` and the bound `MAX_SETTING`;
  `abi::ClientCommandStatus` gains the reason `CHANGE_REFUSED`. The ABI
  stays 1.0 until its release.
