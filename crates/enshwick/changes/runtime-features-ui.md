### Fixed

- Loader: a panic in the HUD or the map markers' work turns off that
  feature and wires the game past its hook; the hook kept running before
  (#1072).
- Loader: map markers a build lacks, and the screen reads, screen
  extensions, open screen names and interaction prompt it lacks while a mod
  reads them, are listed as off on the Mods screen and in the status
  summary, not only in the log (#1072).
- Loader: when the interface or its dialogs turn off, the dialog on screen
  and those waiting show as system message boxes, where they waited for good
  (#1072).
- Loader: when the interface turns off, the menus' Mods entries, the Mods
  screen and the mod control keys turn off with it and are listed off; a
  join refusal no longer goes to a Mods screen that never draws (#1072).
- API: `Ui::set_markers` returns `UiError::Unavailable` while the map
  markers are off, where it reported success (#1072).
- Loader: the HUD stops reading the open screen once a panic turned the
  screen reads off (#1072).

### Changed

- Loader: the main menu's Enshwick line and both menus' Mods entries turn
  off with their parts, at their hooks too (#1072).
