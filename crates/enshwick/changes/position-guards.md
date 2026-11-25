### Added

- `abi::in_fixed_range`, the bound the calls that hand the engine a world
  position check: each axis finite and below 2^31 metres either way, the
  range of the engine's 32.32 fixed point.
- `ecs::checked_fixed` converts metres to fixed point, or gives `None`
  past that range; scripts reach it as `ecs.checked_fixed`.
- `abi::Voxel::PER_METRE`, the voxels per metre that `voxels::PER_METRE`
  now names; ABI 12.27.

### Fixed

- Loader: `HostApi::spawn`, `drop_items`, `teleport`, `weather_at` and
  `ui_markers` refuse a position past the fixed point's range with
  `Status::BAD_ARGS`, where a NaN had landed at 0 and a huge value at the
  range's edge; `build_rights` and `world_persists` refuse a corner past
  it, not only one that is not finite.
- Loader: a placed prop or block is refused at 2^29 metres
  (`Voxel::END` voxels) or more on an axis, the world's voxel range,
  where it had been let through up to 2^30 metres.
- Loader: a server window's map shows its markers in range and leaves off
  one past the fixed point, where one such marker had cleared them all.
- Loader: a server sound placed within 64 metres below 2^31 reaches remote
  clients, which had refused the position as it rounded to `f32`.
- `abi`: `SoundPlay`, `SoundSteer` and `SoundAudience` are in bounds only
  with a position in that range, not every finite one.
- Fake host: refuses each of these positions as the loader does, after
  the same checks the loader runs first, and a placed prop or block
  outside the voxel range.
