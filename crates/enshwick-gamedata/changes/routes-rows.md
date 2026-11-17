### Added

- Shape holes `?1` to `?9`: one register bound across a shape's steps, as
  an operand or a memory base, in any width.
- A shape's `{id}` names a constant entry's value too, such as a reflected
  field offset, as a displacement (`[?+{id}]`) or an immediate.
- A `callees` route: among the functions a function calls directly, the one
  whose code the shape matches.
- Engine routes for a Character screen row's grid cell
  (`ui.attribute_row_cell`, `ui.attribute_row_cell_second`) and the byte
  the submenu's empty row holds 1 in (`ui.attribute_row_hovered_flag`), on
  every kept client.

### Changed

- The inventory put's result routes (`inventory.put_out_*`) match only
  where the three stores go through one base register.
- The Character screen's row builders (`ui.attribute_rows_<group>`) are the
  callee of the submenu that reads the reflected text of their own group,
  no longer the n-th call.
- The compatibility reports of the kept builds hold the changed routes.

### Fixed

- A shape's `{id}` for a constant route stands for that route's value or
  the table's constant, never for a symbol of the same name.

### Removed

- The routes of each builder's first text read
  (`ui.attribute_rows_<group>_text`), of the breakdown flag
  (`ui.attribute_row_breakdown_set`, `ui.attribute_row_breakdown_row`) and
  of the row list's count (`ui.attribute_list_filled`).
