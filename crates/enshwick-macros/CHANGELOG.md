# Changelog

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Each entry names the pull request that made the change.

## [Unreleased]

### Changed

- `#[point]` and `#[event]` fail the build on padding in a point's input or
  answer or an event's data, with the message `#[call]` gives (#587).

### Fixed

- A `#[script]` method whose closure parameter lends its argument (`&T`)
  hands the script a copy instead of failing to compile (#581).

## [1.0.0-alpha.7] - 2026-10-26

Released beside `enshwick` 1.0.0-alpha.7, with no changes of its own.

## [1.0.0-alpha.6] - 2026-10-26

Released beside `enshwick` 1.0.0-alpha.6, with no changes of its own.

## [1.0.0-alpha.5] - 2026-10-26

Released beside `enshwick` 1.0.0-alpha.5, with no changes of its own.

## [1.0.0-alpha.4] - 2026-10-26

Released beside `enshwick` 1.0.0-alpha.4, with no changes of its own.

## [1.0.0-alpha.3] - 2026-10-26

### Added

- `manifest!` takes `was: [...]` for a renamed mod (#369) and
  `points: [...]` for the extension points a mod defines (#394), and checks
  that a mod id has an author part (#378).
- `#[point]` writes `Point::SCHEMA`, and `#[derive(Shape)]` writes the
  schema names and types with their doc comments; `#[point]` refuses a type
  a schema cannot describe (#394).
- `#[config(group)]` for a table of settings (#399) and `#[config(first = n)]`
  for the number of a list's first element (#425).

## [1.0.0-alpha.2] - 2026-10-26

### Removed

- Chat command generation for actions (#336).

## [1.0.0-alpha.1] - 2026-10-26

The first version on crates.io (#347).
