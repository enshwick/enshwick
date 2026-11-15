# Changelog

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Each entry names the pull request that made the change. What mod authors
see is in the changelog of `enshwick`.

## [Unreleased]

## [1.0.0-alpha.17] - 2026-11-15

### Added

- A script reads and writes a pair of `f32` as a list of two numbers, as a
  look or a move's direction (#1118).

### Changed

- `Guid` is the one GUID of the ABI, in the game's byte order (#1106).

## [1.0.0-alpha.16] - 2026-11-15

### Changed

- The docs of `Id`, `is_id`, `Guid::named` and `Guid::try_named` link the book's page on how ids are namespaced.
- Unit tests hold what `Off::to_reason` gives the loader: a text as its key and values, any other reason as its words.

## [1.0.0-alpha.15] - 2026-11-14

### Changed

- **Breaking:** `ResourceType`, `Guid::key`, `Id::key` and the hook that
  answered a type's hash are gone; `ResourceType` lives in `enshwick-core`
  (#1044).
- `Result<T, script::Error>` is an `Answer` for every `T: FromScript`;
  the hidden `Fields` moved to `enshwick-typed` (#1044).

