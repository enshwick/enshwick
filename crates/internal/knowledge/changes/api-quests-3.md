### Added

- `Quests::item_sets` reads every item set of the journal, mods' included,
  as `ItemSetInfo` with its entries, hints, queries, experience, recipes
  and the bool of its pay row.
- `NewRule`, `RuleGrant`, `RuleChange`, `RuleId` and `RuleInfo` describe
  knowledge rules in the game's trigger list: while a query holds, the
  game grants bools and flags.

### Changed

- `quests::EntryId` is `content::journal::EntryId`, re-exported at its old
  path.

### Fixed

- `Quests::state` and `Quests::world_state` take the game's answer for a
  step of the game's journal that asks a game query; before, they gave
  the opposite.
