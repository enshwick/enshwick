### Fixed

- `CommandAction::Equip` of an item moved out of the equipment inventories
  with `MoveItem` puts it back: the game's equipment slot still names that
  item's new slot, so its own request moved the item onto itself and the
  command timed out. Such an equip is now a move back into the equipment
  slot the item left, while that slot is empty and holds nothing else; a
  server mod sees it as an item transfer, not an equip.
