### Added

- ABI 12.13: `HostApi::buff_type_add` with `BuffTypeAdd` adds a buff type of
  a mod's own. The loader serves one patch of the game's buff collection
  with every mod's types.

### Changed

- `Load::add_buff` hands the type to the loader, which builds the buff
  collection once for every mod instead of once per mod. It refuses an id a
  mod loaded before added, at the init of the mod that added it second.
  Unstable.

### Fixed

- A mod buff type shows its own icon on a client that read the buff
  collection before it showed mods' icons. The loader writes the icon into
  the collection the game holds once the client shows them. Unstable.
