### Changed

- `HostApi::serve_resource` refuses a new key whose GUID the calling mod did
  not declare with `declare_content`. A key served from its own GUID, a key
  the game's archive holds and an old GUID of the mod's own renames need no
  declaration.
- `HostApi::terrain_material_add`, `HostApi::crafting_prop` and the new side
  of `HostApi::rename_content` refuse a GUID the calling mod did not
  declare, and warn with the name of the mod that declared it. A rename
  onto the game's content or onto the old GUID of a later rename passes.
  The fake host refuses the same calls. Breaking.
- `HostApi::rename_content` refuses a rename from a GUID another mod
  declared, and renames in a cycle. Breaking.
- `Load::rename_content` takes renames only to content ids the mod
  declared first with `Load::declare`, to the game's content, or to the old
  GUID of a later rename. Breaking.

### Fixed

- `HostApi::terrain_material_add` and `HostApi::serve_resource` answer a
  late call as too late before they look at the GUID.
- The fake host answers a rename after init as too late, as the loader does,
  and keeps each mod's renames apart.
- `Load::rename_content` keeps no rename the loader refused, so an item
  added after a refused rename is served under its own GUID alone.
