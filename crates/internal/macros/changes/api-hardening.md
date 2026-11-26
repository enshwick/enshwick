### Changed

- `#[script]` takes an `impl Into<T>` parameter as `T` from a script
  (#1295).
- **Breaking:** `declare_mod!` builds its manifest through a hidden
  constructor, as `Manifest` is `#[non_exhaustive]` (#1295).
