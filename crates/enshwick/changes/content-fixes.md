### Added

- `DataSelect::selects_items` answers whether a data selection picks items
  (#1190).

### Fixed

- A mod that declares a content id keeps it when a mod loaded earlier renamed
  from its GUID: the loader drops that rename and the resources that mod
  serves under the GUID from another base, a chain of that mod's renames
  through the GUID skips it, and a warning names both mods (#1190).
- Fake host: a content id declared after another mod's rename drops that
  rename and its serves, as in the loader (#1190).
