### Fixed

- A mod's init and ready, a data pack's init and a state dropped after a
  second init catch a panic through `host::caught` and `host::guard`, so a
  panic whose payload panics again on drop no longer reaches the game.
