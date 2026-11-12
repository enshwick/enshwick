### Added

- The crate: the ui domain, the interface and sound, cut out of
  `enshwick-core`, with its `Bound` implementations (#1055).
- `__load`, the method traits of each module that core's `Load` forwards
  to, `__internal`, what core calls here and a mod must not, and
  `__fake`, what the fake loader reads here (#1055).

### Changed

- `enshwick-world` is a dependency only with `unstable`, which the map
  markers and the typed actions need (#1056).
