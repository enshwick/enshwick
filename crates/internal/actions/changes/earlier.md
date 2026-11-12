### Added

- The crate: the actions domain, cut out of `enshwick-core`, with its
  `Bound` implementations (#1054).
- `__load`, the method traits of each module that core's `Load` forwards
  to, `__internal`, what core calls here and a mod must not, and
  `__fake`, what the fake loader reads here (#1054).

### Changed

- `enshwick-inventory` is no longer a dependency: the console's item
  check asks `enshwick-content` (#1056).
