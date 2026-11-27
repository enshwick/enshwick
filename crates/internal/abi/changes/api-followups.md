### Changed

- Its errors derive `script::ErrorKind`, so a mod's `#[script]` method may
  return them (#1300).
- Doc examples build entities with `Entity::from_raw`, never the hidden
  `Entity::from_engine` (#1300).
