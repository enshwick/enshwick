### Fixed

- Loader: every served resource the game holds is followed by zeroed bytes,
  a page or a sixteenth of its size where that is more. The game reads some
  resources past their end, such as `VoxelTemperatureResource`, which it
  reads in whole layers, so a world whose mods served one, even unchanged,
  crashed the client as it loaded.
