### Added

- `World::language_of` gives the language a player reads the game in, for
  a mod's own texts to that player outside an action's request; Luau
  `enshwick.world.language_of` (#1067).

### Fixed

- `World::tell` and `World::notify` log a line for the console or a remote
  caller, which has no account and gets nothing; `notify` answers the new
  `Notice::Nobody` for them instead of `Notice::Chat` (#1067).
