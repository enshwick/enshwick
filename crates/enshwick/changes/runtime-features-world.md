### Fixed

- Loader: the fog of war, its copy, rename filters, chunk events and the
  scene's map markers and frame on a build without their entries show off
  on the Mods screen and in the status summary with the entry they lack,
  not only in the log (#1073).
- Loader: the scene's map markers, the map frame and map marker types
  turn off when their read panics; a scene that lacks its own data, or
  whose read thread does not start, answers missing for that scene only.
  The scene's markers show as world map markers, apart from the
  interface's map markers (#1073).
- Loader: a panic in the fog of war copy turns off the copy alone; the
  uncovers the server asks for run on (#1073).
- Loader: a panic in the time speed link wires the game's speed setter
  past the loader, and `HostApi::set_time_speed` answers
  `Status::MISSING` while the time speed is off (#1073).
- Loader: every world feature arms all its links or none, so a fog of
  war whose reveal fails to link leaves its destroy unpatched (#1073).
