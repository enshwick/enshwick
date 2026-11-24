### Added

- `Archive::uncut_len` gives a resource's whole length from the length the
  game's own map gives it.

### Fixed

- `Archive::resource` reads a resource whole when the archive map stores its
  length cut to its low 24 bits. The map keeps only those bits, so the
  44.66 MB client `VoxelTemperatureResource` part read as its first 11.1 MB.
