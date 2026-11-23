### Added

- A shape gap, `... n` between two steps, lets up to `n` instructions lie
  between them, at most two gaps a shape. A skipped instruction never
  branches and never writes a register the steps before it hold, and when
  a gap skips any, no direct branch of the function lands on one it skips
  or on the step after it.

### Fixed

- The route `net.session_state` skips its join's epilogue, so it no longer
  depends on how many registers the join restores before its tail jump.
