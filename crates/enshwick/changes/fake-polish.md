### Added

- `FakeHost::without_hello(key)` gives a player whose client has not said hello, or has no Enshwick: the windows a mod shows them wait as the loader keeps them, at most `actions::window::MAX_HELD`, the oldest told closed, and `Installed::say_hello(key)` shows them at the hello (#1148).
- `Installed::drive_until(limit, done)` drives the fake server until `done` holds: it writes the queued transactions, then runs one step of `Ticked::DEFAULT_EVERY`, on the fake's own clock (#1148).

### Changed

- The fake host's scheduler counts its waits on the fake's clock, which only its steps move, so a test that waits sleeps no real time (#1148).
- The fake host has no world while a mod's `init` runs, as the loader has none then: the world's calls answer `Status::UNAVAILABLE` there, as players, entities, events, teleports and spawns do after `stop_world` (#1148).
- In the fake host's build of a served resource, a patch that refuses or hands back bytes it was not lent turns its mod off with the loader's reason; a mod's own served read still skips it (#1148).

### Fixed

- `FakeHost::templates()` lists its templates in the game's template collection and serves `template.name`, so `vanilla::template("Enemy_Wolf")` finds the fake's creature and a typo names the near ones (#1148).
- The fake host names a served resource whose builder panicked as the loader does, such as `served item <guid>` (#1148).
