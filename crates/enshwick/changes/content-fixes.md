### Added

- `DataSelect::selects_items` answers whether a data selection picks items
  (#1190).

### Fixed

- A mod that declares a content id keeps it when a mod loaded earlier renamed
  from or to its GUID: the loader drops those renames and what that mod serves
  under the GUID through them, and logs a warning naming both mods (#1190).
- Fake host: a content id declared after another mod's rename drops that
  rename and its serves, as in the loader (#1190).
