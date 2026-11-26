### Fixed

- `CommandAction::MoveItem` names no inventory beside its slots, so the
  server runs the move instead of skipping it.
- `CommandAction::Trash` writes an item transfer of type `Destroy`, which
  the server runs as a plain destroy, instead of the item action `Trash`,
  which it runs only for some items.
