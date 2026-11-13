### Fixed

- A mod's init and ready, a data pack's init and a state dropped after a
  second init catch a panic through `host::caught` and `host::guard`, so a
  panic whose payload panics again on drop no longer reaches the game
  (#1075).
- A data pack's init panic drops its payload while the pack still runs, so a
  second panic there logs as the pack (#1075).
