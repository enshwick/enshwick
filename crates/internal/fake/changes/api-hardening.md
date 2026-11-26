### Added

- `FakeHost::without_world_reads` plays a build whose table lacks the
  world's reads, so a test sees inventory, buff and entity reads fail on the
  game thread (#1295).

### Fixed

- The fake host refuses component, buff and game settings reads off the game
  thread, as the loader does (#1295).
