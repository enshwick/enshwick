### Added

- `World::change_permissions` takes permission bits from a player in game
  and gives others on top of their user group, so the game's own server
  checks refuse what a taken bit guards: `CanEditWorld` digging, bombs,
  building and breaking, `CanAccessInventories` chests in bases. Each call
  replaces the mod's own change and empty lists end it, also for a player
  no longer in game; a bit any mod takes is gone even when another gives
  it; a change ends when its mod turns off, the player leaves or the world
  ends. `Player::permissions` and the loader's own admin checks read the
  bits in force. Unstable, beside `World::can_build`; Luau
  `enshwick.world.change_permissions()` (#1135).
- ABI 12.5: `HostApi::change_permissions` (#1135).
- Loader: the server rebuilds every player's permission byte from their
  user group each frame (server 26430253), so the loader applies the mods'
  changes in its world step link before every step of the server's world,
  under a fuse of its own, one atomic read while no mod changes any
  (#1135).
- Fake host: `Installed::permission_change` reads what every mod asked for
  an account, and `Installed::respawn` moves a player to a new character
  entity (#1135).
