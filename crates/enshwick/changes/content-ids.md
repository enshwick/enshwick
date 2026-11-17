### Changed

- `HostApi::buff_type_add` refuses a GUID whose content id the calling mod
  did not declare with `declare_content`, and warns with both mods' names
  when another mod declared it. `Load::add_buff` declares the id itself.

### Fixed

- A mod item shows its own icon on a client that read the item before it
  showed mods' icons. Once the client shows them, the loader builds every
  served resource again that asked for them before, buff types included,
  and writes the icon into the copy the game holds.
