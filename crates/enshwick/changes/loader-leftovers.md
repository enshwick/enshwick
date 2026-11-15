### Changed

- `Effects::drop_items` answers `WorldError::Missing` when the build lacks
  the item lookup or no item registry or world is there yet, and
  `WorldError::Refused` only for an item the game does not have (#1112).

### Fixed

- Every loader feature that turns off is listed in the status summary and
  on the Mods screen: the server's game events on a build without their
  layout or step, and a loader code patch skipped or given up, which now
  turns off its own feature and ends in a fault code when the game's
  threads kept it out (#1112).
- A panic in one part's start or end of a hosted game turns off that part
  alone, not the world loads and session ends of every part (#1112).
- The status summary and the console's `enshwick mods` answer name a mod
  file refused before its manifest read without a version (#1112).
