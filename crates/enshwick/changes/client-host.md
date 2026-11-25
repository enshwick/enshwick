### Fixed

- `input`: `CommandAction::Host` and `CommandAction::Join` fail at the main menu within a frame or two when the game shows its own error that it could not create the game, instead of `Failed(TimedOut)` after 20 seconds; a host is `Failed(Unknown)`, a join names what the client can tell.
