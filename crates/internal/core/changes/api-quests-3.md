### Added

- `Load::remove_quest` and, with the `unstable` feature,
  `Load::change_journal_page` and `Load::remove_journal_page`, with their
  Luau mirrors in `enshwick.content`.
- `Load::add_item_set` adds an item set to the journal's collections with
  its two queries and a pay row the game pays once;
  `Load::change_item_set` and `Load::remove_item_set` change and remove a
  vanilla set with its pay row; with their Luau mirrors.
- `Load::add_knowledge_rule` adds a rule to the game's trigger list,
  `Load::knowledge_rules` reads the game's, and
  `Load::change_knowledge_rule` and `Load::remove_knowledge_rule` change
  one and turn one off in its place; with their Luau mirrors.
- `Load::add_query` adds a query to the game's query database, which the
  game evaluates as its own, and `Load::game_query` finds one of the
  game's queries by name; with their Luau mirrors.
