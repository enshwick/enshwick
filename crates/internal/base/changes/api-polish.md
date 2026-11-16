### Added

- `Grow::reserve` (#1136).

### Changed

- A served builder's panic answers `Status::FAULTED`, which turns off
  only its resource (#1136).
- The scheduler passes on each game's first step, before its world loads
  (#1136).
