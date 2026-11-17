### Added

- A shape's `?o` memory base: any register but RSP, RBP and RIP, so a
  local read at the same displacement cannot match (`[?o+*]`).
- `callers` and `callees` routes may list several functions: each one found
  is looked through on its own, a match the row's `check` refuses counts
  as none, two matches find nothing, and every one that finds something
  must find the same.

### Fixed

- A store through a 32-bit stack or frame address no longer counts toward
  how far a function writes the object it is handed (`take = "extent"`).

### Changed

- The Character screen's row builders (`ui.attribute_rows_<group>`) match
  their group's text read only through a register off the stack.
- The row push (`ui.attribute_row_push`) is found among the callees of
  every group's builder, no longer only in the Main group's builder, so no
  group's rows depend on another group's route.
- The compatibility reports of the kept builds hold the changed routes.
