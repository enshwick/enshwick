# Changelog

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Each entry names the pull request that made the change. What mod authors
see is in the changelog of `enshwick`.

## [Unreleased]

### Changed

- **Breaking:** `ResourceType`, `Guid::key`, `Id::key` and the hook that
  answered a type's hash are gone; `ResourceType` lives in `enshwick-core`
  (#1044).
- `Result<T, script::Error>` is an `Answer` for every `T: FromScript`;
  the hidden `Fields` moved to `enshwick-typed` (#1044).
