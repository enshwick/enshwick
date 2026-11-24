### Fixed

- Loader: a served resource whose length the game's archive map stores cut
  to 24 bits is built from all of its bytes. The client's 44.66 MB
  `VoxelTemperatureResource` part was served as its first 11.1 MB, and the
  game, which reads it by its own header, crashed as a world loaded.
- A mod's read of such a resource from the game's archive returns all of
  its bytes, not the first 11.1 MB.
