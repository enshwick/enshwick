### Fixed

- Loader: the status summary and the Mods screen list each base feature a
  game build or the game's Steam library turns off, not only the log: game
  event delivery, world keys, the own channel to other machines, the join
  check, the join dialog hold, the join check's kicks, the hosted lobby and
  the mod set advertisement (#1074).
- Loader: a panic in the hosted lobby or the mod set advertisement turns
  that feature off and lists it, where before it was only logged (#1074).
- Loader: a panic in the join check turns it off for the rest of the game
  and lists it; before, that join went on unchecked and the next join was
  checked again (#1074).
- Loader: once the join check is off, a join it lets through shows the
  game's own "could not join" dialog again (#1074).
- Loader: on a build without the event stream layout, a client still gets
  its server's notices and frees what it handed the game (#1074).
