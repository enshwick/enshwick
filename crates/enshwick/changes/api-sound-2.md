### Added

- `sound::play` starts a sound container on a client in 2D, at a place,
  on an entity it follows or as music, from any thread, and returns a
  `SoundHandle` that stops it, fades its volume, moves it and reads its
  `SoundState`; `sound::stop_all` stops every sound of the mod. A mod's
  sounds end when it is turned off, and its world sounds when the
  client's world ends. Luau `enshwick.sound.play{...}` and
  `enshwick.sound.stop_all`.
- ABI 12.17: `SoundPlay`, `SoundSteer`, `SoundInfo` and the `sound_play`,
  `sound_steer` and `sound_state` host functions; `SoundPlay::in_bounds`
  and `SoundSteer::in_bounds` hold their bounds for the loader and the
  fake host alike.
- Fake host: `Installed::sounds` and `FakeSound` list the sounds a mod
  started; the next `draw_frame` starts the queued ones.
- `Load::add_voiceover` (unstable) adds a voiced line from one audio file
  set per voice language, with a fallback language for the slots it leaves
  empty; `Load::change_voiceover` gives a game line new recordings in some
  languages and keeps the others. `NewVoiceover`, `VoiceLine`,
  `VoiceoverId`. Luau `enshwick.sound.add_voiceover` and
  `change_voiceover`.
- `Load::add_music` (unstable) adds recordings and a music row of the
  client's ambience: by environment, fog, indoors, battle and time of day,
  with its volume and pause; rows of several mods add up in load order.
  `NewMusic`, `MUSIC_VOLUME`, `DAY`. Luau `enshwick.sound.add_music`.
- Data packs: `[[voiceover]]` and `[[music]]` tables in `sounds.toml`.
