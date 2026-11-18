### Fixed

- Terrain materials: the end of a joined session takes the provider's state
  with no session from the game build, so a build that numbers the
  provider's states differently turns off only the switch back to the
  client's own terrain ids, with a message naming the build, instead of
  switching at the wrong moment.
