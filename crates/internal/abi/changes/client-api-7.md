### Added

- `ClientCommand::REBIND`, `ClientCommand::SET_SETTING`, `ClientCommand::QUIT`,
  the fields `ClientCommand::key` and `ClientCommand::key_slot`,
  `ClientCommand::MAX_SETTING` and `ClientCommandStatus::CHANGE_REFUSED`.

### Changed

- Until the ABI's release, a layout change keeps ABI 1.0 and rewrites the
  golden file; from the release on, an append takes the next minor.
