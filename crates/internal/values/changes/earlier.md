### Changed

- **Breaking:** `ResourceType`, `Guid::key`, `Id::key` and the hook that
  answered a type's hash are gone; `ResourceType` lives in `enshwick-core`
  (#1044).
- `Result<T, script::Error>` is an `Answer` for every `T: FromScript`;
  the hidden `Fields` moved to `enshwick-typed` (#1044).
