### Added

- `Load::add_buff` and `buffs::NewBuff` add a buff type of a mod's own: a
  copy of a vanilla type in the game's buff collection with its own name,
  description, icon, default life time, slot and flags, and `plain` to drop
  what the vanilla type runs. A mod's types are served on both sides in one
  patch with the collection's hash map rebuilt; `add_buff` refuses at init a
  server-only mod, a `like` the game lacks, an id the game has and an id the
  mod added before; Luau `enshwick.buffs.add_buff()`. Unstable (#1153).
- Fake host: `FakeHost::buffs` sets the buff collection's layout and puts a
  vanilla collection of the given types in the archive (#1153).
- The `lucky` example adds a buff type with its own icon, name and luck
  modifier, given by a control on L (#1153).
