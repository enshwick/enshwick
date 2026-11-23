### Added

- `SoundChange` (unstable) sets every value of a sound container, each bounded: `scene`, `has_position`, `sleep` and `sleep_randomness`, `avoid_repeating_last`, `loop_same_take`, `max_count`, `stealing_mode`, `random_position`, `mute_on_main`, `gamepad_speaker` and `gamepad_vibration`.
- `SoundChange::takes` replaces or adds a container's takes (`Takes`, `Take`), each a recording named by the new `SoundId`, in load order across mods, at most 64 per container; an unknown recording is refused at init.
- `Load::replace_sound` (unstable) plays a mod's file wherever the game plays one of its recordings; the last mod in load order wins and the log names the one it replaced.
- `Load::add_sound` reads WAV (8, 16, 24 and 32-bit, and float), FLAC, Ogg Vorbis and MP3 at any rate, trims it (`Trim`), maps its channels (`Channels`), resamples it to 48 kHz, scales it by a gain and encodes it as the game's `FramelessQOA` on background threads: inline up to `INLINE_FRAMES`, streamed past it.
- `NewSound` takes 1 to `MAX_NEW_TAKES` takes (`NewTake`), picked at random by their chances.
- A pack's `sounds.toml` takes every container value, a sound's takes, channels, gain and trim, and `[[change]]` and `[[replace]]` tables; Luau has `enshwick.sound.replace`, and `enshwick.sound.change` and `enshwick.sound.add` take the same fields.

### Changed

- `Load::add_sound` returns `AddedSound`, the container and its recordings. Breaking.
- `NewSound` takes `takes` instead of `file`, and its scene and other container values in `container`. Breaking.
- `SoundChange` is no longer `Copy`. Breaking.
- Mod sounds are held as `FramelessQOA` instead of uncompressed, and a WAV file no longer has to be 16-bit at 48 kHz.
