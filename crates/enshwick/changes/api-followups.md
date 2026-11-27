### Added

- `script::ErrorKind` names the kind a script reads from an error; derive it
  beside `thiserror::Error` (#1300).

### Changed

- **Breaking:** a `#[script]` method's error type implements
  `script::ErrorKind`, which names the error's kind, or is a `script::Error`
  with a kind of its own; the kind no longer comes from the error's `Debug`
  text (#1300).
- Every public error of the API derives `script::ErrorKind`, so a mod's
  `#[script]` method may return it (#1300).
- **Breaking:** `test::Installed::hover_slot` takes the item's own entity
  as an `Option<Entity>` (#1300).
- Doc examples build entities with `Entity::from_raw`, never the hidden
  `Entity::from_engine` (#1300).

### Fixed

- A script error from an `io::Error` has its `io::ErrorKind`'s name as its
  kind, such as `NotFound`, not `Os` or `Custom` (#1300).
