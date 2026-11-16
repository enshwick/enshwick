### Added

- `Interaction::moved`, `Interaction::transfer`, `interactions::Move`,
  `interactions::Transfer` and `interactions::InventoryActionType`: what a
  player's move moves and its kind, with the `unstable` feature.

### Changed

- With the `unstable` feature, `interactions::Interaction` has a new public
  field, `moved`: code built with it that writes an `Interaction` as a struct
  literal names `moved: None`.
