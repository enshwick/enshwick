### Added

- With the `typed` feature, `Quests::start`, `advance`, `complete`, `reset`
  and `step` move a quest on for a `Holder`, a player or the world: each
  grants or revokes exactly the knowledge the steps test, once per entry in
  a call, and refuses with `MoveQuestError::Unwritable` a step that only a
  game query moves.
- With the `typed` feature, a quest step's objectives are counted on the
  events the mod already gets: pickups, kills, crafts, placed items,
  positions once a second and `Quests::signal` for custom ones, with one
  subscription per kind of event and one table lookup per event.
- The Luau mirror: `enshwick.quests.start`, `advance`, `complete`, `reset`,
  `step` and `signal`, with a holder of `"World"` or `{ Player = player }`.

### Changed

- `rewards::Action::Craft` and quest objectives credit a craft by one rule,
  with one step clock per mod.
