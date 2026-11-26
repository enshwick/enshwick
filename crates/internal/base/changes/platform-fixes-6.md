### Fixed

- In a build with the fake host's features, the scheduler counts its delays
  and periods on the fake's clock only once a fake host is installed, and on
  the wall clock before, so the loader's tests see time pass even when
  another crate of their build turns those features on.

### Added

- `__internal::fake_clock`: whether the scheduler counts time by a fake
  host's steps, so the loader's tests can check that theirs is the wall
  clock.
