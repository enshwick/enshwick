### Added

- `game_events::NotifyEvent`, the typed notify event, with `NotifyEventType` and `SoundVolumeHint`.
- A `Field` reads and writes a `Guid`.
- `peer::MAX_NOTIFIES_PER_STEP` and `peer::PerStep`, the one per-step bound of a mod's calls the loader and the fake host share.
