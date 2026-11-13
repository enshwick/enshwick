### Fixed

- `#[hook]` catches its body's panic through `host::caught`, so a panic
  whose payload panics again on drop no longer reaches the game.
