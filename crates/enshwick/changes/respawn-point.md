### Added

- Unstable `World::respawn_point` and `World::set_respawn_point` read and move where a player respawns: a spawn point entity with its `PlayerSpawnType` and transform. A move raises the game's own change event, as the game does.
- Unstable typed game event `PlayerSpawnInfoChangeEvent`, with the character, and the base or the save point's scene as a `UniqueSceneId`.
- ABI 12.12: `RespawnPointInfo`, `HostApi::respawn_point` and `HostApi::set_respawn_point`.
- `test`: `FakeHost::spawn_point`, `FakeHost::spawn_point_names` and `FakeHost::respawns_at` place spawn points and respawn points; a set raises the change event as the loader does, and `Installed::respawn_point_moved` raises it as the game does.
