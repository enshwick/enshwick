# Changelog

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Each entry names the pull request that made the change.

## [Unreleased]

## [1.0.0-alpha.3] - 2026-10-26

### Added

- `cargo enshwick point <mod>:<point> --from <dll|folder|schema>` writes
  the Rust definition of another mod's extension point from the owner's DLL,
  with compile-time checks against its layout (#394).

### Changed

- `cargo enshwick new` takes an `<author>.<name>` id and names the crate
  and folder after the name (#378).
- Errors carry context: each names the file or step it was working on (#379).

## [1.0.0-alpha.2] - 2026-10-26

No changes of its own. Released with `enshwick`.

## [1.0.0-alpha.1] - 2026-10-26

The first version on crates.io (#347).
