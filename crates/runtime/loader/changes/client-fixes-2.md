### Fixed

- `CommandAction::MoveItem` names no inventory beside its slots, so the
  server runs the move instead of skipping it.
- `CommandAction::Trash` writes an item transfer of type `Destroy`, the
  request the game's trash sends, instead of the item action `Trash`, which
  the server runs only for some items.
