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
