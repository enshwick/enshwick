### Added

- `sound::play` (unstable; no client has yet been seen playing it) starts
  a sound container on a client in 2D, at a place,
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
- Fake host (unstable): `Installed::sounds` and `FakeSound` list the sounds a mod
  started; the next `draw_frame` starts the queued ones.
- `Load::add_voiceover` (unstable; no client has yet been heard playing
  it) adds a voiced line from one audio file
  set per voice language, with a fallback language for the slots it leaves
  empty; `Load::change_voiceover` gives a game line new recordings in some
  languages and keeps the others. `NewVoiceover`, `VoiceLine`,
  `VoiceoverId`. Luau `enshwick.sound.add_voiceover` and
  `change_voiceover`.
- `Load::add_music` (unstable; no client has yet been heard playing it)
  adds recordings and a music row of the
  client's ambience: by environment, fog, indoors, battle and time of day,
  with its volume and pause; rows of several mods add up in load order.
  `NewMusic`, `MUSIC_VOLUME`, `DAY`. Luau `enshwick.sound.add_music`.
- Data packs: `[[voiceover]]` and `[[music]]` tables in `sounds.toml`.
- `sound::play_for` (unstable; a server copy sends it, no client has yet
  been seen playing it) plays a sound container from a mod's server part on
  the clients that run Enshwick and the mod: for one player, for everyone
  or near a place, at most 32 per mod in one server step; a hosting
  client's own player hears it too. The `RemoteSound` it returns stops,
  fades and moves the sound on those clients. Luau
  `enshwick.sound.play_for`.
- ABI 12.18: `SoundAudience` and the `sound_play_for` and
  `sound_steer_for` host functions; `SoundAudience::in_bounds` and
  `SoundAudience::hears` hold the audience's rules for the loader and the
  fake host alike.
- Fake host (unstable): `Installed::sent_sounds` lists the sounds a mod sent clients
  and to whom; `Installed::hear_sound` takes one as a client does.

### Fixed

- A joined client reads a server mod's settings again: the stat values of
  the Character screen and the settings snapshot shared one control
  number, so the client read every snapshot as stat values and its Mods
  screen showed no server settings. The control messages move to version
  4, so a loader of an earlier release reads none of them and a join with
  mods of side both gets the line naming what the player needs.
