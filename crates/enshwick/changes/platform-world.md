### Added

- `abi`: `Event::PROGRESS_WRITTEN` and `ProgressWriteData`, ABI 11.25.
  After the level step, the mod that asked for a level or experience write
  hears whether it was applied, with the level and experience it left, or
  why it was dropped (#1064).

### Changed

- Loader: the item drop's pickup is read on a thread of its own as the
  world loads, so the first drop no longer holds a server frame; only a
  drop asked for during that read, about a tenth of a second, waits for
  the rest of it (#1064).
