### Added

- Routes for the node map lookup the interface binds actions through
  (`ui.node_find`, `ui.context_nodes`), the slot widget's pointer test
  (`ui.item_slot_pointer`) and whether the gamepad drives the interface
  (`ui.context_input`, `ui.input_navigating`), on every kept client.

### Changed

- `ui.item_slot_hovered` is `ui.item_slot_focused`: the byte follows the
  focus, not the cursor.
