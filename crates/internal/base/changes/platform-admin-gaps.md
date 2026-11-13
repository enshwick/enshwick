### Added

- `config::set` and `TableConfig::set` write a value of the mod's own world
  settings, checked as an edit is and raised as a change, with
  `SetSettingError` for a refusal; Luau `enshwick.config.set` (#1068).
