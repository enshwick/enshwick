### Added

- `script::Bound`, whose `resolve` takes `&mut impl Loading`, and the
  base's own `Bound` implementations beside their types (#1053).
- The crate: what every domain of the API shares, cut out of
  `enshwick-core` (#1046).
- `BaseLoad`, the base handle `Load` holds, and `Loading`, which the
  functions below `Load` take for the mod's loader (#1046).
- `resources::serve`, the served-resource builder, and the host's resource
  reads, moved down from the content domain (#1046).
