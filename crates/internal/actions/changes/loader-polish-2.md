### Changed

- A client command with an argument the build does not know, such as an
  equipment slot, screen or game action name, returns
  `WorldError::BadArgument` instead of `WorldError::Refused` (#1306).
