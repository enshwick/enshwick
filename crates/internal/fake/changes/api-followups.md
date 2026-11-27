### Changed

- **Breaking:** `hover_slot` takes the item's own entity as an
  `Option<Entity>` (#1300).
- Doc examples build entities with `Entity::from_raw`, never the hidden
  `Entity::from_engine` (#1300).
