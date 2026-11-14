# Changelog

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Each entry names the pull request that made the change. What mod authors
see is in the changelog of `enshwick`.

## [Unreleased]

## [1.0.0-alpha.15] - 2026-11-14

### Added

- Register explicitly scoped client-world event observations without server-world authority (#1078).
- `script::Bound`, whose `resolve` takes `&mut impl Loading`, and the
  base's own `Bound` implementations beside their types (#1053).
- The crate: what every domain of the API shares, cut out of
  `enshwick-core` (#1046).
- `BaseLoad`, the base handle `Load` holds, and `Loading`, which the
  functions below `Load` take for the mod's loader (#1046).
- `resources::serve`, the served-resource builder, and the host's resource
  reads, moved down from the content domain (#1046).
- `config::set` and `TableConfig::set` write a value of the mod's own world
  settings, checked as an edit is and raised as a change, with
  `SetSettingError` for a refusal; Luau `enshwick.config.set` (#1068).
- `events::ProgressWritten`: what became of each level or experience write
  the mod asked for, heard after the level step; Luau
  `enshwick.on("ProgressWritten")` (#1064).
- `host::caught` and `Panicked`, for a mod's callback whose panic turns
  off only the part that panicked; `host::guard`, points, served calls and
  the hook macro catch through it (#1062).

### Changed

- `load::client_game_events_of` needs the `unstable` feature and takes 0 for
  every type (#1081).
- `i18n::language` names building events among the calls about one
  player (#1067).

### Fixed

- A mid-function hook's handler, a served resource's builder and a read of
  the manifest catch a panic through `host::caught`, so a panic whose
  payload panics again on drop no longer reaches the game (#1075).
- A mid-function hook's panic faults the mod with the panic's message after
  the hook's address (#1075).

