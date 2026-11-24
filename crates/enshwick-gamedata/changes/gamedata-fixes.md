### Added

- `SteamBuild::installed` reads the Steam build of a game folder from the
  appmanifest Steam or SteamCMD keeps for it, and `Side::steam_app` names
  each side's Steam app.
- `Catalogue::snapshot_for` writes a snapshot whose first line records its
  build's Steam build, and `snapshot_steam_build` reads it back.

### Changed

- `newest_snapshot` returns a `NewestSnapshot` and ranks snapshots of
  builds without a kept table by the Steam build they record, not by file
  write time; a snapshot that records none ranks below every one that
  does and is listed as unranked.
- `snapshot_build` takes the Steam build the snapshot records and names it.
