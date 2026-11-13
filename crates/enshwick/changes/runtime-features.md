### Added

- Loader: error code E1010 names a loader feature that turned itself off
  while the game ran; the log line, the status summary and the Mods screen
  name the feature (#1062).

### Fixed

- Loader: a feature arms all its detours or none. When one of its links
  fails to link or to arm, its other links are dropped or wired past, and
  once a feature turns off the game runs past every detour of it. A
  feature that turns off logs once and is listed in the status summary and
  on the Mods screen (#1062).
- Loader: a mod's panic whose payload panics again when dropped no longer
  aborts the game from a served call, a point or a detour (#1062).
