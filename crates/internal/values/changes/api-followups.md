### Added

- `script::ErrorKind`, with kinds for `Off` and `io::Error` (#1300).

### Changed

- **Breaking:** `script::Error::failed` takes an error that implements
  `script::ErrorKind` and reads its kind from it, `BadArgs` for an empty
  one (#1300).
- A `#[script]` method may return a `script::Error`, which keeps its own
  kind (#1300).
