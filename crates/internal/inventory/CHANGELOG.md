# Changelog

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Each entry names the pull request that made the change. What mod authors
see is in the changelog of `enshwick`.

## [Unreleased]

## [1.0.0-alpha.15] - 2026-11-14

### Added

- The inventory domain's `Bound` implementations, beside their types
  (#1053).
- The crate: the inventory domain, cut out of `enshwick-core` (#1052).
- `__load`, the method traits of each module that core's `Load` forwards
  to, and `__internal`, what core calls here and a mod must not (#1052).

### Changed

- `Effects::drop_items` no longer holds the first drop's server frame:
  the loader reads the pickup as the server starts, before its world
  loads, and a drop before that read ends waits for the rest of it (#1064).

### Removed

- **Breaking:** the hidden `__internal::knows_item`, now in
  `enshwick-content` (#1056).

