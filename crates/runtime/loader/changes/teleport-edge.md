### Fixed

- `HostApi::teleport` refuses a target outside the box the world's scene
  keeps players in with `Status::OUTSIDE_WORLD`, where a player sent to the
  2^31 metre edge of the fixed point had crashed the joined client. The box
  comes from the scene's `SceneResource`, read from the game's archive once
  per scene after the first server step; a teleport before that read ends
  answers `Status::NOT_LOADED`. Where the box does not read (a build without
  its layout, a scene a mod serves), only the fixed point bounds a teleport,
  and an entity that is gone answers `Status::MISSING` before its target is
  checked (#1299).
