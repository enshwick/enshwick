### Fixed

- `CommandAction::MoveItem` and `Trash` run on the server; before, it
  skipped both. `CommandAction::Equip` says that an item of another kind
  than the equipment slot is refused.
