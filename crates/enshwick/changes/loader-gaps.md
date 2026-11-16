### Added

- `resources::Output`, the bytes the loader lends a served resource's
  builder, and `Registry::encode_into`, which writes a value into them
  (#1124).

### Changed

- `Load::serve`'s builder takes the base and its `Output`, writes the
  resource there and returns `Option<()>`, so a build allocates once and
  copies nothing out; a builder that built bytes elsewhere ends with
  `out.copy_from(&bytes)`; an `Output` left empty fails the build as
  `None` does (#1124).
- Opening the input with `Load::input`, or a Luau mod's start, starts the
  local player's held reads, so `local_held` answers in the mod's first
  frame in a game (#1124).
- The fake host answers `local_held` only after a draw that follows the
  first ask, as the loader does (#1124).
- The loader reads each archive chunk once when it lists the vanilla items
  that selections pick from: about 50 s per game build in a debug build,
  0.5 s now (#1124).

### Fixed

- `Map::uncover` and `Map::uncover_all` uncover the fog of the player who
  hosts a game on their client; they failed with `Unavailable` (#1124).
- On a game build without the event streams, the craft filters and the
  item event filters are listed off with the entry the build lacks, beside
  the game event delivery, and by the event move when it does not link
  (#1124).
- The fake host reports the local input as requested once opening the
  input or a `local_held` ask started the reads, as the loader does
  (#1124).
