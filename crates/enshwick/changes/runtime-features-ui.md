### Fixed

- Loader: a panic in the HUD or the map markers' work turns off that
  feature and wires the game past its hook; the hook kept running before
  (#1072).
- Loader: map markers, screen reads, screen extensions, open screen names
  and the interaction prompt that a build lacks are listed as off on the
  Mods screen and in the status summary, not only in the log (#1072).
- Loader: dialogs waiting when the interface or its dialogs turn off show
  as system message boxes, where they waited for good (#1072).
- Loader: the HUD stops reading the open screen once a panic turned the
  screen reads off (#1072).

### Changed

- Loader: the main menu's Enshwick line and both menus' Mods entries turn
  off with their parts, at their hooks too (#1072).
