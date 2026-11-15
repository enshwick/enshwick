### Added

- A development build of the loader states a version no release takes,
  `<release>.dev+<commit>`, such as `1.0.0-alpha.16.dev+5f647f46`, on the
  main menu line, in the start log, the console's `enshwick` line and the
  status summary; the log, the console line and the status name the ABI
  version beside it (#1114).
- The loader finds the client's input builder, the HUD's own screen toggle,
  the in-game menu's tab and the crafting menu's recipe browser on every
  kept client build, and turns that part off with a log line on a build
  that lacks one (#1114).

### Fixed

- A held-controls read that went stale stays apart from one never made,
  and the loader logs once why a step found no mask: no row passed the
  interface's local player check, or the row had no player input (#1114).
