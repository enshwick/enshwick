### Fixed

- Loader: the fog of war copy, rename filters and chunk events on a build
  without their entries show off on the Mods screen and in the status
  summary with the entry they lack, not only in the log (#1073).
- Loader: map markers, the map frame and map marker types turn off when
  their reflected layout is missing or their read panics; a scene that
  lacks its own data answers missing for that scene only (#1073).
- Loader: a panic in the time speed link wires the game's speed setter
  past the loader, and `HostApi::set_time_speed` answers
  `Status::MISSING` while the time speed is off (#1073).
- Loader: every world feature arms all its links or none, so a fog of
  war whose reveal fails to link leaves its destroy unpatched (#1073).
