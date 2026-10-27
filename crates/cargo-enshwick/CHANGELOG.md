# Changelog

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Each entry names the pull request that made the change.

## [Unreleased]

### Added

- `cargo enshwick new --data` shows a selection of items by category in
  the commented `items.toml` (#558).
- `cargo_enshwick::luau::definitions` writes the API's Luau definitions
  (#543); `cargo enshwick luau` writes them to a file, and
  `cargo enshwick new --luau` starts a Luau mod (#545).

## [1.0.0-alpha.7] - 2026-10-26

Released beside `enshwick` 1.0.0-alpha.7, with no changes of its own.

## [1.0.0-alpha.6] - 2026-10-26

### Fixed

- `cargo enshwick point` refuses a point schema whose names, types or
  `repr` would leave their place in the generated definition, so another
  mod's DLL cannot put code into the extending mod's crate (#512).

### Added

- A library target, `cargo_enshwick::points`, reads schemas from DLLs and
  text and writes definitions, with a typed `SchemaError` (#512).

## [1.0.0-alpha.5] - 2026-10-26

### Added

- `cargo enshwick new --data` starts a data pack (#495).

## [1.0.0-alpha.4] - 2026-10-26

Released beside `enshwick` 1.0.0-alpha.4, with no changes of its own.

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
