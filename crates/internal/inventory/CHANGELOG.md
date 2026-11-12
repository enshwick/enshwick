# Changelog

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Each entry names the pull request that made the change. What mod authors
see is in the changelog of `enshwick`.

## [Unreleased]

### Added

- The crate: the inventory domain, cut out of `enshwick-core`.
- `__load`, the method traits of each module that core's `Load` forwards
  to, and `__internal`, what core calls here and a mod must not.
