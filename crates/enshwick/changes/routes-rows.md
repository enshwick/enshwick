### Fixed

- A stat kind's Character screen row shows from the first frame the screen
  is open in every group, also in an empty group whose builder runs before
  the Main group's: the loader builds the row itself instead of copying a
  game row, and links a group's builder only when a kind shows in it.
- A build that reorders the Character screen's groups still gives each
  group's rows to its own builder, found by the text the builder reads.
- Two groups whose builders a build finds at one function show no rows,
  with the reason, instead of rows under the wrong header.
- The Character screen's rows no longer turn off when a build moves the row
  list's count, which the loader does not read.
