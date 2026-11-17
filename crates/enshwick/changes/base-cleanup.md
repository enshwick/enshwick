### Added

- `abi::LocalInputInfo::EMPTY`, the value a local input read starts from:
  its header set, no flag and never sampled (`age_ms` is `u64::MAX`).

### Fixed

- `effects::Effects::teleport_player_to` answers `WorldError::Unavailable`
  instead of `WorldError::Gone` on a game build that does not reflect
  `CurrentTransform`, when both players are connected.
