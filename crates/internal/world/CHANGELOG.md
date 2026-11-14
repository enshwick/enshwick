# Changelog

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Each entry names the pull request that made the change. What mod authors
see is in the changelog of `enshwick`.

## [Unreleased]

## [1.0.0-alpha.15] - 2026-11-14

### Added

- Add `typed::EventType::on_client` while retaining server event behavior (#1078).
- The world domain's `Bound` implementations, beside their types (#1053).
- The crate: the world domain, cut out of `enshwick-core` (#1050).
- `__load`, the method traits of each module that core's `Load` forwards
  to, and `__fake`, what the fake loader and core's tests read here
  (#1050).
- `World::language_of` gives the language a player reads the game in, for
  a mod's own texts to that player outside an action's request; Luau
  `enshwick.world.language_of` (#1067).
- `World::stop` stops a dedicated server the way Ctrl+C does, final save
  included, for a timed restart; Luau `enshwick.world.stop` (#1068).
- `Map::uncover` and `Map::uncover_all` uncover a player's fog of war
  through the loader on the player's client; Luau `enshwick.map.uncover`
  and `uncover_all` (#1068).

### Changed

- `typed::EventType::on_client` never runs its handler on a dedicated
  server, where it answers without an error (#1081).
- `World::set_level`, `set_experience` and `add_experience`: the mod hears
  each write's outcome after the level step through `ProgressWritten`
  (#1064).

### Fixed

- `World::tell` and `World::notify` log a line for the console or a remote
  caller, which has no account and gets nothing; `notify` answers the new
  `Notice::Nobody` for them instead of `Notice::Chat` (#1067).

