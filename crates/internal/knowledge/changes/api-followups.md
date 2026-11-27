### Changed

- Its errors derive `script::ErrorKind`, so a script reads the same kinds
  without the error's `Debug` text (#1300).
- Doc examples build entities with `Entity::from_raw`, never the hidden
  `Entity::from_engine` (#1300).
