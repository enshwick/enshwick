### Changed

- **Breaking:** option structs and records are `#[non_exhaustive]`, the
  engine layouts leave the stable surface, `ItemSetCategory::ALL` is
  `&'static [Self]` and `changes::SetFieldError` names its error (#1295).
