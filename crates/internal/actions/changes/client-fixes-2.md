### Changed

- `CommandAction::Trash` is an item transfer of type `Destroy`, not an item
  request: a server mod refuses it in its interaction filter, not with an
  item filter on `Trash`. `CommandAction::Equip` says that an item of
  another kind than the equipment slot is refused.
