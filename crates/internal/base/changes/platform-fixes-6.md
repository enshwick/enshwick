### Added

- `__internal::is_fake_host`: whether this build is the fake host's, whose
  scheduler counts time only as the fake's steps move it, so the loader's
  tests can check that they run on the game's clock.
