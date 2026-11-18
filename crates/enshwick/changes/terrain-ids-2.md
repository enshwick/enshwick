### Fixed

- Terrain materials: once a game a client joined ends, the client serves
  its own terrain ids again, with the mods that game turned off back on, so
  a world it hosts next names its mods' materials even when the loader does
  not see the hosted game start.
- Terrain materials: a panic while taking a host's terrain ids no longer
  keeps a client from serving its own again; while serving its own is off,
  a client takes no host's ids.

### Changed

- Terrain materials: a client that serves no terrain material builds the
  palette worlds, effects and looks as copies of the game's, without
  decoding them; on client 26430625 a build of the 145 palette worlds took
  40.7 ms.
