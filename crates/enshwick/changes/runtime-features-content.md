### Fixed

- Loader: mod texts, mod textures, mod templates, disabled recipes,
  recipe output rates, the knowledge guard, experience filters, and the
  data changes of containers, loot, recipes, crafting
  menu stations and selected items and recipes that a build lacks are
  listed as off on the Mods screen and in the status summary, not only in
  the log (#1076).
- Loader: a panic in the loot rate turns off the loot rate alone, not the
  factors of every game rate (#1076).
- Loader: a stand-in item or the data change of one item, container or
  loot table whose build panics is refused alone and listed as off, where
  before the build was skipped without a word; the others keep their
  changes (#1076).
- Loader: once disabled recipes turn off, the crafting menus and the
  craft veto stop applying the last world's rules (#1076).

### Changed

- Loader: served resources and mod textures each arm all their hooks or
  none; the key listing belongs to mod templates, so a fault in the
  textures leaves served templates listed (#1076).
- Loader: the knowledge guard checks the game's code at its site before it
  hooks it (#1076).
