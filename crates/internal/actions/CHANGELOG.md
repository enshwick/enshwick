# Changelog

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Each entry names the pull request that made the change. What mod authors
see is in the changelog of `enshwick`.

## [Unreleased]

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

