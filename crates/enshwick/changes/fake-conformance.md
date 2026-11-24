### Added

- `test::FakeHost::other_mod` names a second mod beside the mod under test, so a test calls the table as that mod.
- `test::FakeHost::without_altar_code` plays a game build without the altar code: asking for the altars and adding an altar zone filter answer `Status::UNAVAILABLE`.
- `test::FakeHost::level_table` sets the level cap and what each level needs for the fake's level step.

### Changed

- The fake host reads the caller of every call first, as the loader does: a handle the loader never gave out answers `Status::BAD_ARGS`, and a mod that is off answers `Status::FAULTED` to most calls; it still logs and reads its fault, settings, folders, language, symbols, constants, resources, the mods, missing mods and what is off. A handle that is neither the mod's, a pack's nor another mod's no longer counts as a data pack of side `server`.
- The fake host checks each argument the way the loader does, before the call does anything, so a null pointer the call needs answers `Status::BAD_ARGS` first.
- The fake host's level step applies the level and experience writes at the next `tick` and raises `ProgressWritten` for the mod that wrote; a write for a character in no level step is heard as `NO_LEVEL_STEP` at the second tick, and the 257th waiting write answers `Status::BUSY`.
- `test::Installed::player_left` also removes the player from the game's players.

### Fixed

- The fake host answers `send_event`, `net_send`, `stat_watch`, `set_config`, `recipe_rule` and `item_data_get` as the loader does: a send before the events are sealed reaches no one, a message only goes on a channel the mod opened, and a watch, settings write, recipe rule or item data read checks what the loader checks, in its order.
- A second mod named with `other_mod` has only what it asked for itself: sending the mod's events, writing levels without asking, reading altars it did not open and writing the mod's settings answer as the loader answers.
- The fake host's level step lets each gain go once the step ends, answers a level or experience write with no world served `Status::UNAVAILABLE`, and as a hosted game ends drops the writes handlers ask for and starts the next game's characters afresh.
