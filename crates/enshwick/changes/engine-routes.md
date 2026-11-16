### Fixed

- A stat kind with a group shows its Character screen row even while the
  game's own list of that group is empty, such as Fog Survival outside the
  Shroud.
- Mods' item slots, the hovered inventory slot, the Character screen's stat
  rows and the container rules turn off, with the entry in the log, on a
  game build that moves a field they share with the game, instead of
  writing to the old place. A Character screen group whose builder the
  build swaps with another group's shows no rows. The hovered inventory
  slot needs only the fields it reads, and a build without the Fog Survival
  builder's breakdown routes keeps every group's rows but those of an empty
  group.

### Changed

- The `item-ui` example also defines Calm in the Fog Survival group.
