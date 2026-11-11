# Changelog

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Each entry names the pull request that made the change. What mod authors
see is in the changelog of `enshwick`.

## [Unreleased]

### Added

- The crate: what every domain of the API shares, cut out of
  `enshwick-core` (#1046).
- `BaseLoad`, the base handle `Load` holds, and `Loading`, which the
  functions below `Load` take for the mod's loader (#1046).
- `resources::serve`, the served-resource builder, and the host's resource
  reads, moved down from the content domain (#1046).
