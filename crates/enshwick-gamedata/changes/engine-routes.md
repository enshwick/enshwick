### Added

- An `extent` take: how far the function an `in` shape lies in writes the
  object it is handed, the end of its highest store off the stack.
- Engine routes for the item slot widget's answer bytes and extent, its
  stack record's count and size, and the extent its slot data init writes
  (`ui.item_slot_*`), on every kept client.
- Engine routes for the result record of the inventory's put of one stack
  into one slot (`inventory.put_out_error`, `inventory.put_out_placed`,
  `inventory.put_out_left`), on every kept build.
- Engine routes for the first text each Character screen row builder reads
  (`ui.attribute_rows_<group>_text`) and the reflected text that opens each
  group (`ui.attribute_text_<group>`), and for the row and row list fields
  the game reads (`ui.attribute_row_*`, `ui.attribute_list_*`), on every
  kept client.

### Changed

- The compatibility reports of the kept builds hold the new routes.
