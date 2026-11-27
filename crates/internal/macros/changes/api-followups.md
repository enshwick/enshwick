### Added

- `#[derive(ErrorKind)]` names an enum's variant or a struct's type as the
  kind a script reads, a raw name without its `r#`, and derives on an enum
  without variants (#1300).
