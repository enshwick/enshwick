### Fixed

- Loader: a mod skipped at start for another mod (a requirement, a
  conflict, a cycle) is listed in the status summary and in the console's
  `enshwick mods` with its file and reason, as on the Mods screen (#1066).
- Loader: a second file of a mod id, which was only logged, shows on the
  Mods screen, in the status summary and in `enshwick mods` with its file
  and a reason that says another copy loads instead; it is not counted as
  off while that copy runs (#1066).
- Loader: the Mods screen's icon buttons take the word that picks their
  look from the game build's route instead of a fixed offset; on a build
  where it is not found, or where it or the icon would overlap another
  button field or lie past the part of the config the game takes, they
  show their text (#1066).
- Loader: a row leaves the scrollbar's width free at its right only in a
  list that scrolls, not in one that pages (#1066).
