### Fixed

- Terrain materials: a client that joins a game takes the terrain ids its
  host serves before it connects: a dedicated server lists them in the Steam
  rules `enshwick.terrain`, a hosting client in its lobby. A client whose
  clone record differs from the host's, or that never had a removed mod,
  now draws that mod's ground as the host's fallback instead of drawing
  nothing or another material.
- Terrain materials: a client running a mod that adds terrain and that the
  server lacks, loaded before a terrain mod the server runs, is refused a
  dedicated server's join with a dialog that names the extra mods (error
  E3002), since the later mod's material would have another id there; so
  is a client that cannot take the server's terrain ids. In a hosted game
  the client joins and reads a note.
- Terrain materials: each world records the materials it was served in
  `<stem>-mod-data/enshwick/terrain.toml`, and a dedicated server places the
  removed mods' materials the worlds of its save folder held first, so a
  world's own ground keeps a free id when more materials are recorded than
  ids are free.
