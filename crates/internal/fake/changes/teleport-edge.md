### Added

- `FakeHost::world_box` sets the box the world's scene keeps entities in; a
  teleport past it answers `Status::OUTSIDE_WORLD`, as the loader does.
  Without it the fake keeps the game's box, 0 to 10,240 m across and 0 to
  2,560 m high, so a teleport there that the game refuses fails in a mod's
  test too (#1299).
