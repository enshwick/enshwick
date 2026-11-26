### Changed

- `CommandAction::MoveItem` names no inventory beside its slots, so the
  server runs the move.
- `CommandAction::Trash` writes an item transfer of type `Destroy`, which
  the server runs as a plain destroy, in place of the item action `Trash`,
  which it runs only for some items.
