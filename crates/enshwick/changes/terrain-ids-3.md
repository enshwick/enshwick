### Fixed

- Terrain materials: a client whose join ends before play starts, because
  the connection fails, the server is full, the password is wrong, the host
  kicks it or the player backs out, serves its own terrain ids again, with
  the mods that join turned off back on. The client no longer waits for a
  hosted start to do so, and the switch also works while the interface is
  off.
