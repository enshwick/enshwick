### Added

- `abi::in_fixed_range`, the one bound every call that takes a world
  position checks: each axis finite and below 2^31 metres either way, the
  range of the engine's 32.32 fixed point.
- `ecs::checked_fixed` converts metres to fixed point, or gives `None`
  past that range; scripts reach it as `ecs.checked_fixed`.

### Fixed

- Loader: `HostApi::spawn`, `drop_items`, `teleport`, `weather_at`,
  `ui_markers`, `build_rights` and `world_persists` refuse a position past
  the fixed point's range with `Status::BAD_ARGS`, where a NaN had landed
  at 0 and a huge value at the range's edge.
- `abi`: `SoundPlay`, `SoundSteer` and `SoundAudience` are in bounds only
  with a position in that range, not every finite one.
- Fake host: refuses each of these positions as the loader does, and a
  placed prop or block at `Voxel::END` metres or more on an axis.
