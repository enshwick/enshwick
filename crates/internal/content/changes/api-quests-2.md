### Added

- With the `typed` feature, `QuestStep::objectives` lists what the journal
  counts on a step (`Objective`: collect, kill, craft, build, reach and
  custom), each on a counter the step's `progress` shows.

### Changed

- `QuestStep::shown_when` and `done_when` are optional. A step without
  `done_when` gets a bool of the loader's, and one without `shown_when`
  shows once the step before it is done, so `Load::add_quest` gives a mod
  quest knowledge that `Quests` moves on exactly. `QuestStep::new` takes the
  two queries as before, and `QuestStep::owned` leaves both to the loader.
- `QuestStep` and `NewQuest` are no longer `Eq`, since an objective holds a
  place in metres.
