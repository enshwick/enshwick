### Fixed

- A mid-function hook's handler, a served resource's builder and a read of
  the manifest catch a panic through `host::caught`, so a panic whose
  payload panics again on drop no longer reaches the game.
