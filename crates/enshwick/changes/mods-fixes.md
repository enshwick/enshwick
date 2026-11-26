### Added

- `altars`: `CellBox::grown` widens a zone on every horizontal side,
  `CellBox::footprint_touches` tells whether two footprints meet whatever
  their heights, and `CellBox::union_footprint_area` counts the cells
  that several footprints cover together.
- `content`: `workshops::FLAME_ALTAR` names the Flame Altar's own
  workshop, where the recipes that claim an altar and raise its level are
  made.

### Fixed

- `actions`: one run of an action asks its `at` once. Before, a
  per-player action that named a player asked it twice per run, so a mod
  that acts in `at` acted twice.
- `altars`: `Load::limit_altar_level` finds the altar's upgrade recipes by
  the altar's workshop, never by guessing that the workshop of the first
  upgrade from level 0 is the altar's. A fake host's
  `upgrade_recipes` for the altar now name the workshop
  `workshops::FLAME_ALTAR`.
