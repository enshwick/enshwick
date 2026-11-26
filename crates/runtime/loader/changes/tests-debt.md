### Fixed

- The game event subscriptions seal in the seal phase, not in their part's
  link, so no other part's link sees them open (#1296).

### Changed

- Every timer of the loader reads one millisecond clock (#1296).
- A registry first sealed while the loader links its parts logs where, and
  fails a debug build there (#1296).
- Tests that need the game's files are ignored by default and fail without
  their variable when run with `--run-ignored` (#1296).
