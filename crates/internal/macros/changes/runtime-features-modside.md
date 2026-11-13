### Fixed

- `#[hook]` catches its body's panic through `host::caught`, so a panic
  whose payload panics again on drop no longer reaches the game (#1075).
- `#[hook]` and `detour!` run their body through one helper, so both keep
  the same panic policy (#1075).
