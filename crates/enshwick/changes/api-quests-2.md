### Added

- With the `typed` feature, `quests::Quests` starts, advances, completes and
  resets a quest for a player or the world (`quests::Holder`), vanilla
  quests and mods' alike, on the game's own knowledge.
- With the `typed` feature, quest steps take objectives the journal counts
  (`content::journal::Objective`), and `quests::Quests::signal` counts a
  mod's own.

### Changed

- `content::journal::QuestStep::shown_when` and `done_when` are optional:
  a step left to the loader gets knowledge of its own
  (`QuestStep::owned`).
