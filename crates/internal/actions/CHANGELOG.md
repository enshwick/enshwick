# Changelog

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Each entry names the pull request that made the change. What mod authors
see is in the changelog of `enshwick`.

## [Unreleased]

## [1.0.0-alpha.17] - 2026-11-15

### Added

- `Input::local_status()`, `Input::client_snapshot()` with its
  `highlighted_recipe()`, and `Input::client_command()`,
  `cancel_command()` and `command_status()` with `CommandAction`,
  `CommandState` and `CancelReason`, and their Luau mirrors (#1118).

### Changed

- `Arg::template` and `Value::Template` name a template, which a window
  lists by name; a run answers `ActionCall::ALLOWED` from its rights alone,
  and carried controls follow those rights (#1106).

## [1.0.0-alpha.16] - 2026-11-15

### Added

- `Actions::on_refused`, `Refused` and `RefusedReason`, and the script's
  `on_refused` (#1094).

### Changed

- Commands and console lines map the loader's statuses through the shared
  status rule (#1083).
- Every reader of a typed line and of the interface's values lives in one
  module, and a value is checked once at the boundary (#1094).
- `Registry::allowed` runs once per run (#1094).

### Fixed

- Answer an interface run whose text is longer than its argument takes with the argument's bound instead of a refusal without a reason (#1085).
- Arguments from the interface that do not decode answer with the usage, not
  the text of a missing right (#1094).
- `Action::admins` is the permission to kick and ban, so it holds beside an
  `allow` (#1094).

## [1.0.0-alpha.15] - 2026-11-14

### Added

- The crate: the actions domain, cut out of `enshwick-core`, with its
  `Bound` implementations (#1054).
- `__load`, the method traits of each module that core's `Load` forwards
  to, `__internal`, what core calls here and a mod must not, and
  `__fake`, what the fake loader reads here (#1054).

### Changed

- `enshwick-inventory` is no longer a dependency: the console's item
  check asks `enshwick-content` (#1056).
- A chat filter's panic warning names the panic's message; the chat filter
  and the typed lines catch through `host::caught` (#1062).

### Fixed

- An item argument's GUID is checked exactly for a mod's item too, not only
  by the game id it shares (#1068).

