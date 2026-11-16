### Added

- Build filters learn the builder's account: `Placement::account` in Rust
  and Luau, and `BuildCheck::account` appended to the ABI as minor 12.10. The
  loader copies each player's account before every server step, so a filter
  judges a player whose character changed from the next step, without a world
  read. A client's own prediction gets no account, in the fake host too.

### Fixed

- A fall that a damage filter makes lethal kills the player: the game
  records the death and `PlayerDied` comes with the cause `FallDamage`. A
  lethal fall that a filter spares records no death. The fall's own death
  decision now follows the health the filters left.
- The fake host runs build filters as on the game's worker threads, where
  world reads answer `WRONG_THREAD`, and its player reads refuse off the
  game thread, as the loader's do.
