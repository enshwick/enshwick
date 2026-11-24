### Added

- `QuestChange` sets every journal field of a vanilla quest: its title,
  source, kind, main mark, priority, tutorial mark and document, beside its
  rewards and who it unlocks for.
- `StepEdit` changes, inserts, removes and moves a vanilla quest's steps in
  order; an inserted step takes an entry id no other entry holds.
  `StepChange` sets every field of a step: title, text, queries, progress,
  map marker, voiceover, icon and level.
- `Load::remove_quest` removes a vanilla quest, refused when the journal
  builds while a step of another mod's quest asks for its id itself.
- With the `unstable` feature, `Load::change_journal_page` (`PageChange`)
  and `Load::remove_journal_page` change and remove a vanilla lore page.
- `QuestStep::marker` points the journal's "show on map" at a map marker,
  and `QuestStep::voiceover` plays a voiceover; `NewQuest` takes
  `priority`, `tutorial` and `document`.
- `content::journal::EntryId` names a journal entry.
- `NewItemSet`, `ItemSetEntry`, `ItemSetHint`, `ItemSetChange`,
  `ItemSetCategory` and `ItemSetId` describe the journal's item sets, and
  `journal::item_set_category` names the game's 20 categories: an entry is done on its
  item's pickup flag unless it names a query, and a set completes on an
  all-of query of its entries and shows on an any-of one.
- `NewQuery` and `QueryForm` describe a query of a mod's own for the game's
  query database: parts joined by all, any or one of two, inverted when it
  asks.

### Changed

- Breaking: `QuestChange::steps` is a list of `StepEdit`; a step change is
  `StepEdit::Change(StepChange { step, .. })`, and `StepChange` has a field
  for every part of a step.
- Breaking: `StepChange` is no longer `Copy`, since it holds a list of
  progress queries, and `QuestChange` is no longer `Eq`, since an inserted
  step is not.
- Breaking: `KnowledgeTest::Fails` is gone, and `KnowledgeQuery::not` of a
  game query is `None`. The game reads a game query's answer alone and no
  compare, so a step written as `Fails` held where the query held.

### Fixed

- A game query of the game's journal reads as `KnowledgeTest::Holds`
  whatever its compare. Every one of them compares with 0, which read as
  `Fails` before, and `KnowledgeQuery::game` writes the same form.
