### Changed

- `World::set_level`, `set_experience` and `add_experience`: the mod hears
  each write's outcome after the level step through `ProgressWritten`
  (#1064).
