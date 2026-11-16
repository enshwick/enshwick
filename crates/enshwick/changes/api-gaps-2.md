### Added

- `World::when_settled` runs a handler once a transaction stopped running:
  a step after it is written whole (applied, or expired meanwhile), when its
  placed props stand, or once it ended otherwise or a bound passed, with a
  `Settled` of its status and props; Luau has
  `enshwick.world.when_settled` (#1130).
- The fake host's `expire_transactions` drops the undo data of written
  transactions, as the loader does at its memory cap (#1130).
- `Blob::over` edits a blob in a served resource's `Output`, and
  `resources::Grow` names the byte stores a blob and the content helpers
  build in (#1130).

### Changed

- The content helpers `clone_item`, `list_item`, `clone_station`,
  `list_workshop`, `list_recipes`, `move_recipes`, `add_recipe_to`,
  `derive_recipes_in`, `resize_container` and `clone_image` build into an
  output passed as their last argument and return `Option<()>`, which is
  `#[must_use]`, so a served build copies no vector out (#1130).
- `Stats::watch` is per mod: an entity stays watched while any mod watches
  it, so one mod's stop leaves another's watch, and a mod turned off loses
  its watches (#1130).
- Opening the input starts the client snapshot's reads too, so
  `client_snapshot` answers in a mod's first frame in a game; the fake host
  answers it only after a draw that follows the first ask (#1130).
- Opening the world subscribes the mod's scheduler tick, which waits for
  transactions (#1130).
