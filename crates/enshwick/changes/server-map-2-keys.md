### Fixed

- Loader: the game's map key opens the map under a window from the server
  that holds a map, with whatever key, mouse button or gamepad button the
  player bound to it. The loader adds the map's keys to the keys the game
  lets through its windows while such a window shows, and gives the game
  its own keys back once the window closes, steps aside for the map or a
  field of it has the caret. Other game keys stay held under the window.
