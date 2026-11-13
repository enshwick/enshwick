### Fixed

- Loader: a mod skipped at start (an unmet requirement, a refused side, a
  second copy) is listed in the status summary and in the console's
  `enshwick mods` with its file and reason, as on the Mods screen. A
  second copy of a mod that runs is listed but not counted as off (#1066).
- Loader: the Mods screen's icon buttons take the word that picks their
  look from the game build's route instead of a fixed offset; on a build
  where it is not found, they show their text (#1066).
- Loader: a row leaves the scrollbar's width free at its right only in a
  list that scrolls, not in one that pages (#1066).
