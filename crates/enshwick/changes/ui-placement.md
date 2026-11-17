### Added

- ABI 12.15: `UiExtension` carries `align`, `shift` and `inset`, which place
  a screen panel along its edge and away from it, and
  `UiExtension::placement_holds` checks them. A mod of an older minor keeps
  its panel centered at its edge.
- `Load::extend_screen` and `Load::extend_screen_for` take a
  `ui::PanelPlacement` beside an `ui::Edge`, so a panel can sit clear of the
  screen's own controls; Luau takes a placement table. Unstable.
- `FakeLoader::screen_placements` says where each panel of a screen sits.

### Changed

- Luau's `extend_screen` and `extend_screen_for` take a placement table,
  `{ edge = "Right" }`, where they took an edge name. Unstable.

### Fixed

- `Frame::hovered_slot` and a mod's slot's `hovered` follow the mouse cursor
  instead of the slot the player selected; with a gamepad they name the
  focused slot. Unstable.
- Back on a loader dialog binds only to a popup node the game holds, so the
  client no longer faults when the game names the popup's window another
  way.
