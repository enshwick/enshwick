### Added

- Routes find the session's join message handler (`session.join_game`), its
  reply send (`session.join_reply`), its machine lookup
  (`session.find_machine`), the join message's account list, the
  banned reply's flags and the session's peer slots on every kept server and
  client build, with checks on the three functions' first instructions
  (#1100).

### Changed

- The compatibility reports count the 17 new routed entries (#1100).
