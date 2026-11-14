### Changed

- The book's stability chapter names every call and type behind the `unstable` or `typed` feature, such as `Load::add_sound`, `Load::limit_altar_level` and the content calls that take typed items; `cargo xtask check` fails when a gated public item is missing from it.
- The book explains why a game update needs no Enshwick update (`concepts/game-updates.md`), lists what a mod gets from Enshwick in its introduction, and says what a Luau call costs.
- The page on how ids are namespaced covers mod commands: the name from the mod's id, the alias rules and what the loader refuses.
