### Added

- `abi`: `HostApi::language_of`, ABI 11.27 (#1067).
- Loader: the server gives each player's language, as their game named it
  when they joined, to `HostApi::language_of` (#1067).
- Loader: `i18n::language()` gives the language of the player whose
  character placed or removed the building while a mod handles
  `BuildingPlaced` or `BuildingRemoved` (#1067).
- `test`: `FakeHost::player_language`, read by `World::language_of` and by
  a building event of that player's character (#1067).
- `test`: `FakeHost::pack` names a data pack's side, by which the fake
  refuses the pack's rates, live changes, crafting props and net channels
  as the loader does; a handle neither the mod's nor named counts as a
  pack of side server (#1067).

### Fixed

- A data pack with `side = "both"` declares a live change of a field
  players' games read and a rate clients count, as a mod of that side does
  (#1067).
- The loader no longer logs a selection of a mod that is off as changing
  vanilla items, nor counts its items as changed, nor warns per item for
  its fields (#1067).
- Loader: a language a client names for its action replies counts only
  when the game knows it, and every reader of a player's language sees the
  same one; a building event of a player whose game runs no Enshwick reads
  the fallback, also in a hosted game (#1067).
- `test`: the fake keeps the reader's language around a nested event and
  puts it back after a panic, as the loader does (#1067).
