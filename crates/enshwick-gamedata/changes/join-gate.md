### Added

- Routes find the session's join message handler (`session.join_game`), its
  reply send (`session.join_reply`), the join message's account list, the
  banned reply's flags and the session's peer slots on every kept server and
  client build, with checks on the two functions' first instructions (#1100).

### Changed

- The compatibility reports count the 16 new routed entries (#1100).
