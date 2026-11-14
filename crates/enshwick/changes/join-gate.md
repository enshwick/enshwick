### Added

- `Load::on_join` asks a mod's rule for each account that joins the server,
  before its player enters the world; a refused account gets the game's own
  reply for an account on the ban list, so its player reads that the host
  denies access and loads no world, and the ban list stays as it was. Luau
  `enshwick.world.on_join` (#1100).
- ABI 11.30: `HostApi::join_rule`, `JoinCall` and `JoinRuleFn` (#1100).
- Fake host: `Installed::join` asks the mod's join rules as the server does
  (#1100).
- Loader: the join check links the game's join message handler only when a
  mod has a join rule, and turns only the join rules off, listed on the Mods
  screen, on a build where an entry it needs is missing; the per-mod profile
  times join rules (#1100).
