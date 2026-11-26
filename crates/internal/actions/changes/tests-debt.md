### Changed

- A mod whose actions take a template reads the vanilla template names on a
  thread of its own as the world loads, not on the game thread at the first
  console line that names one (#1296).
