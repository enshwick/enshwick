### Added

- A shape gap, `... n` between two steps, lets up to `n` instructions lie
  between them; a skipped instruction never branches, and no direct branch
  of the function lands in the gap.

### Fixed

- The route `net.session_state` skips its join's epilogue, so a build that
  saves another number of registers still finds the state with a session.
