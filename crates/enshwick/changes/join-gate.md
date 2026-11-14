### Added

- `Load::on_join` asks a mod's rule for each account that joins the server,
  before its player enters the world; a refused account gets the game's own
  reply for an account on the ban list, so its player reads that the host
  denies access and loads no world, and the ban list stays as it was. A
  machine that already joined is never refused. On a build without the join
  path `on_join` fails at init, so the mod can choose another way. Luau
  `enshwick.world.on_join` (#1100).
- ABI 11.30: `HostApi::join_rule`, `JoinCall` and `JoinRuleFn` (#1100).
- Fake host: `Installed::join` asks the mod's join rules as the server does
  (#1100).
- Loader: the join check links the game's join message handler only when a
  mod has a join rule, keys accounts as the game does, reads a join without
  allocating and logs refusals at most every 10 seconds; the per-mod profile
  times join rules (#1100).
