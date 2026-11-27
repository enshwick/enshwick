### Changed

- Its errors derive `script::ErrorKind`, so a script reads the same kinds
  without the error's `Debug` text (#1300).
- Doc examples build entities with `Entity::from_raw`, never the hidden
  `Entity::from_engine` (#1300).

### Fixed

- `CommandAction::Trash` says an item filter on trash refuses it too, as the
  backpack's Delete (#1300).
- `CommandAction::Equip` says an item a `MoveItem` took out of the equipment
  moves back into the equipment slot it left, heard as an item transfer
  (#1300).
