### Fixed

- Terrain materials: the end of a joined session takes both the provider's
  state with a session and the one with none from the game build, so a build
  that renumbers the provider's states keeps the switch back to the client's
  own terrain ids, or turns off only that switch with a message naming the
  state the build lacks.
