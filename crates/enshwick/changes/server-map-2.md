### Fixed

- Loader: a window from the server that holds a map draws an Open the map
  button under its map line, which now only says what the window marks
  there. The window takes the game's own keys while it
  shows, so the map key did not open the map and its markers could not be
  used. The button stays off on a build that cannot open the Map screen,
  and a press the game's queue refuses is tried again for about a second.
