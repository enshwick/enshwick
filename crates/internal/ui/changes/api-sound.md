### Added

- Every value and the takes of a sound container in `SoundChange`, `Load::replace_sound`, and mod sounds from WAV, FLAC, Ogg Vorbis and MP3 encoded as `FramelessQOA`, with several takes each.

### Changed

- `Load::add_sound` returns `AddedSound`, `NewSound` takes `takes` and `container`, and `SoundChange` is no longer `Copy`. Breaking.
