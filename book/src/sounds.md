<!--
type: how-to
description: A mod plays and changes sound containers, replaces the game's recordings, and adds its own sounds, voiced lines and music from audio files.
-->

# Sounds

A sound container is what the game plays for one event: a set of sounds
and how it plays them. A mod names one by its GUID, a
[`SoundContainerId`](https://github.com/enshwick/enshwick/blob/main/crates/internal/ui/src/sound.rs),
and plays it, changes it or adds its own.

## Play a container on a client

With the `unstable` feature, `sound::play(Play::new(id, place))` starts a
container on the client that runs the mod. Only that client hears it. It
may change in any release: no client has yet been seen playing it. The
place is one of four:

| Place | Where it plays |
| --- | --- |
| `Place::Screen` | From no place, as the game's interface sounds |
| `Place::At(position)` | At a point in the world, in metres; it pans and fades as the camera moves |
| `Place::On(entity)` | On an entity of the client's world, following it; it ends with a short fade once the entity is gone. The client follows at most 64 entities, every mod's together; past that a sound on a new entity fails with `FollowFull` |
| `Place::Music` | As music; a new music sound of the same mod fades out its earlier one |

`.volume(v)` sets a volume from 0 to 1, which the game multiplies by the
container's own. `.fade_in(duration)` fades it in, for at most 60 seconds.

The call works from any thread, in a frame or not. It queues the sound,
and the client's next frame starts it, so a handle reads `Pending` until
then. One mod starts at most 64 sounds a frame. A container that neither
the game's archive nor a mod holds is refused at once. On a server the call
fails.

The `SoundHandle` it returns steers that sound:

| Call | Does |
| --- | --- |
| `stop(fade)` | stops it, fading out |
| `set_volume(v, fade)` | changes its volume, fading |
| `set_position(at)` | moves a sound in the world; a sound on an entity no longer follows it, and a screen or music sound does not move |
| `is_playing()`, `state()` | tell where it stands |

Once the sound ended, every call on its handle changes nothing.
`sound::stop_all(fade)` stops every sound of the mod.

A mod's sounds end when the mod is turned off. Its sounds at a place or on
an entity also end when the client's world ends. Its interface sounds and
music keep playing then.

```rust
use core::time::Duration;
use enshwick::content::Guid;
use enshwick::sound::{self, Place, Play, SoundContainerId, SoundHandle};

const BELL: SoundContainerId = SoundContainerId::from_guid(Guid::named("you.bells:bell"));

fn ring(at: [f64; 3]) -> Result<SoundHandle, enshwick::Off> {
    sound::play(Play::new(BELL, Place::At(at)).volume(0.6).fade_in(Duration::from_millis(200)))
}
```

```luau
local BELL = "8a5c1d07-3c3e-5f0b-9a8e-0c41e3f9d2a7"
local bell = enshwick.sound.play({ container = BELL, place = { At = vector.create(10, 0, 0) }, volume = 0.6 })
bell:stop(2)
```

A sound that misleads players in a fight, such as the steps of an enemy
that is not there, changes how the fight plays. Say so in the mod's
description.

Inside a frame of the interface, `frame.play(id)` also plays a container as
an interface sound. The [interface chapter](interface.md) lists it with the
widgets.

## Play a sound on players' clients from the server

With the `unstable` feature, `sound::play_for(audience, play)` plays a
container on clients from a mod's server part. It may change in any
release: a server copy sends it, but no client has yet been seen playing
it. Each client that runs Enshwick and the mod plays it as
the mod's own `sound::play` would. A client without Enshwick hears
nothing; for a sound every client hears, raise a notifier
([Raise a notifier at a place](#raise-a-notifier-at-a-place)). The mod
runs on both sides.

| Audience | Who hears it |
| --- | --- |
| `Audience::Player(key)` | The client of one player |
| `Audience::Everyone` | Every client |
| `Audience::Near { at, radius }` | The clients whose character stands within the radius of the place when it starts; no radius means 500 metres, as far as any sound carries |

The server's entity ids name no entity of a client's world. So
`Place::On(entity)` plays where the entity stands when the sound starts,
and does not follow it. A near audience and `Place::On` read where things
stand, so they work only on the server's game thread, as in a step or an
event. The other calls work from any thread. One mod sends at most 32
sounds in one server step. The server does not know which containers a
client holds: a client that lacks the container drops the sound and logs
it once.

In a game a client hosts, the host's own player hears the sound too.

The `RemoteSound` it returns steers the sound on the clients it went to.
It stops, fades and moves the sound as a `SoundHandle` does. Clients send
nothing back, so it has no state to read. A change reaches only the
mod's 256 newest sounds.

```rust
use enshwick::content::Guid;
use enshwick::sound::{self, Audience, Place, Play, RemoteSound, SoundContainerId};

const HORN: SoundContainerId = SoundContainerId::from_guid(Guid::named("you.horns:horn"));

fn sound_the_horn(at: [f64; 3]) -> Result<RemoteSound, enshwick::Off> {
    sound::play_for(Audience::Near { at, radius: Some(80.0) }, Play::new(HORN, Place::At(at)))
}
```

```luau
local HORN = "8a5c1d07-3c3e-5f0b-9a8e-0c41e3f9d2a7"
local horn = enshwick.sound.play_for({ Near = { at = vector.create(0, 0, 0), radius = 80 } }, { container = HORN, place = { At = vector.create(0, 0, 0) } })
horn:stop(2)
```

## Change a vanilla sound

`load.change_sound(id, change)` sets the values a
[`SoundChange`](https://github.com/enshwick/enshwick/blob/main/crates/internal/ui/src/sound.rs)
gives and keeps the rest. The client applies every mod's changes in load
order when it first reads the container. Nothing is written to the
game's files or a save. A server checks the values and does nothing more,
since servers play no sound.

| Value | Bound |
| --- | --- |
| `volume`, `volume_randomness` | 0 to 4 |
| `pitch` | 0.25 to 4 |
| `pitch_randomness` | 0 to 4 |
| `min_distance`, `max_distance` | 0 to 500 metres, the least not past the greatest |
| `looped`, `has_position`, `loop_same_take`, `mute_on_main` | true or false |
| `priority` | 0 to 65535 |
| `scene` | Sfx, Music, Ui or Voice |
| `sleep`, `sleep_randomness` | 0 to 10 minutes, the pause between two plays of a looped container |
| `avoid_repeating_last` | 0 to 64 takes |
| `max_count` | 0 to 8192 plays at once, 0 for no limit |
| `stealing_mode` | which play a new one stops at the limit: Oldest, Furthest, or Ignore the new one |
| `random_position` | 0 to 500 metres to either side and forwards; both 0 turn the spread off |
| `gamepad_speaker` | 0 (off) to 4 |
| `gamepad_vibration` | 0 (off) to 1 |
| `takes` | see the next section |

```rust
use enshwick::content::Guid;
use enshwick::sound::{SoundChange, SoundContainerId};

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let bell = SoundContainerId::from_guid(Guid::named("you.bells:bell"));
    let mut sound_change = SoundChange::default();
    sound_change.volume = Some(0.2);
    load.change_sound(bell, sound_change)
}
```

```luau
enshwick.sound.change("<container GUID>", { volume = 0.2 })
```

## Change which recordings a container plays

A container picks one of its takes at random each time it plays. A take is
a recording, a [`SoundId`](https://github.com/enshwick/enshwick/blob/main/crates/internal/ui/src/sound.rs),
and its weight among the other takes, 0 to 1. A change's takes either
replace every take the container holds (`Takes::Replace`) or follow them
(`Takes::Add`). Each mod's takes apply in load order, so an addition
follows the takes of the mods before it. A container holds at most 64
takes.

A take names one of the game's recordings by its GUID, or one a mod loaded
before declared, such as the recording `add_sound` returns. The client
refuses any other GUID at init, naming it in the log.

```rust
use enshwick::content::Guid;
use enshwick::sound::{SoundChange, SoundContainerId, SoundId, Take, Takes};

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let chest = SoundContainerId::from_guid(Guid::named("you.bells:chest"));
    let creak = SoundId::from_guid(Guid::named("you.bells:creak.sound"));
    let mut change = SoundChange::default();
    let mut take = Take::new(creak);
    take.chance = 0.5;
    change.takes = Some(Takes::Add(vec![take]));
    load.change_sound(chest, change)
}
```

```luau
enshwick.sound.change("<container GUID>", {
    takes = { Add = { { sound = "<recording GUID>", chance = 0.5 } } },
})
```

## Replace a vanilla recording

With the `unstable` feature, `load.replace_sound(id, file)` plays the
mod's file wherever a container plays the game's recording. The file follows the rules of [`add_sound`](#add-a-sound-of-your-own).
Mods that replace one recording apply in load order: the last one's file
plays, and the log names the one it replaced. A server checks the file and
holds nothing.

```rust
use enshwick::content::Guid;
use enshwick::sound::SoundId;

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let creak = SoundId::from_guid(Guid::parse("4f1d6a3e-90b2-4c7e-8d15-2a6f0c93b7e1"));
    let _replaced = load.feature("creak", |load| load.replace_sound(creak, "sounds/creak.wav"));
    Ok(())
}
```

```luau
enshwick.sound.replace("<recording GUID>", "sounds/creak.wav")
```

## Hear the sounds the server places

The server places sounds in the world through notify events. A hit, a
pickup or a survivor arriving each raises one, and every client in range
plays it. `load.on_notify(handler)` calls the handler on the server's game
thread for each one. Its
[`NotifyEvent`](https://github.com/enshwick/enshwick/blob/main/crates/internal/base/src/game_events/notify.rs)
names what plays in `type_guid`. For the type `Normal` that is a notifier.
For the type `Sfx` it is a sound container. The visual effects, type
`Vfx`, are not handed on.
`load.on::<NotifyEvent>` hears all three types. On a client the handler
hears only a game that client hosts.

```rust
fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    load.on_notify(|notify| {
        enshwick::debug!("{:?} {:?} at {:?}", notify.r#type, notify.type_guid, notify.position);
    })
}
```

```luau
enshwick.sound.on_notify(function(notify)
    enshwick.log.info(`{notify.type} {notify.type_guid} at {notify.position}`)
end)
```

## Raise a notifier at a place

`effects.notify(&Notify::new(notifier, at))` raises a notifier at a point
or on an entity, as the game's own systems do. Every client in range plays
its sound, effect and camera shake after the next step, a client without
mods too. A notifier is a `keen::NotifierResource`, named by its GUID.
`sound_volume_hint` says whose sound it is, so the client's volume
settings group it; the default is any sound effect.

A mod raises at most 32 notifiers in one server step
(`MAX_NOTIFIES_PER_STEP`). The call past that bound is refused, and so is
entity 0. A position not finite or at 2^31 metres or more on an axis is
refused too. The call belongs on the server's
game thread, in a handler, never at the top of `init`. A notifier that
misleads players in a fight does not belong in a mod.

`effects.notify` needs the `unstable` feature and may change in any
release. No client has yet been seen playing a notifier the server raised.

```rust
use enshwick::content::Guid;
use enshwick::effects::{Notify, NotifyAt};
use enshwick::game_events::SoundVolumeHint;

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let effects = load.effects();
    let chime = Guid([7; 16]);
    load.on::<enshwick::events::PlayerJoined>(move |joined| {
        let notify = Notify::new(chime, NotifyAt::Entity(joined.entity))
            .sound_volume_hint(SoundVolumeHint::FriendlyNpc);
        if let Err(why) = effects.notify(&notify) {
            enshwick::debug!("no chime: {why}");
        }
    })
}
```

```luau
enshwick.on("PlayerJoined", function(joined)
    local ok, why = pcall(enshwick.effects.notify, {
        notifier = "<notifier GUID>",
        at = { Entity = joined.entity },
        sound_volume_hint = "FriendlyNpc",
    })
    if not ok then
        enshwick.log.info(`no chime: {why}`)
    end
end)
```

## Choose which sound a notifier plays

A notifier's `NotifierResource` names the container it plays in
`defaultSound`. Other game data names one too, such as an ambient entry's
`soundContainer`. A mod changes which container plays by serving a changed
[resource value](resource-values.md#serve-a-change). Any container's GUID
fits there, a vanilla one or one from `add_sound`.

```rust
use enshwick::resources::{Registry, Value};
use enshwick::sound::SoundContainerId;

const NOTIFIER: &str = "keen::NotifierResource";

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let bell = SoundContainerId::from_guid(enshwick::content::Guid::named("you.bells:bell"));
    let registry = Registry::game()?;
    let ty = registry.named(NOTIFIER).ok_or_else(|| enshwick::Off::new("no notifiers"))?;
    let Some(key) = load.host().resource_keys(Some(ty))?.into_iter().next() else {
        return Ok(());
    };
    load.serve(key, key, move |base, out| {
        let mut notifier = registry.decode(NOTIFIER, base).ok()?;
        *notifier.field_mut("defaultSound")? = Value::Guid(bell.guid());
        registry.encode_into(NOTIFIER, &notifier, Some(base), out).ok()
    })
}
```

```luau
local key = enshwick.resources.resource_keys("keen::NotifierResource")[1]
enshwick.resources.serve(key, key, function(notifier)
    return { defaultSound = "<container GUID>" }
end)
```

## Add a sound of your own

With the `unstable` feature, `load.add_sound(new)` adds audio files of the
mod's folder as recordings, and a container that plays them. A `NewSound`
holds up to 32 takes, each a file and its chance. The container picks one
take at random by their chances each time it plays.

`add_sound` returns the container and the recordings, in take order. Other
containers' takes name the recordings. The container's GUID is the
[content id](concepts/ids.md)'s, so other mods play it by
`you.horns:horn`. The recordings are `you.horns:horn.sound`,
`you.horns:horn.sound2` and on. The new container's values follow its
`SoundChange`, whose takes may only add to the files'. A data pack lists
its sounds, changes and replaced recordings in
[`sounds.toml`](content/packs.md#soundstoml).

Every file of a sound goes through the same steps at init:

1. It is read: WAV (8 to 32-bit, or float), FLAC, Ogg Vorbis or MP3, at
   8 to 384 kHz, with 1 to 8 channels. Reading stops at the trim's end.
2. The trim keeps the part from its start to its end in the file.
3. The channels setting keeps the file's channels (`Keep`) or mixes them
   to one (`Mono`). `Stereo` takes two. The game plays 1, 2, 4, 6 and 8
   channels. An Ogg Vorbis file's channels are put in the order WAV and
   FLAC files keep them. A sound placed in the world wants mono: the game
   plays every channel from the same point.
4. It is resampled to 48000 Hz, the rate of every sound the game plays.
5. The gain scales it, from -60 to 24 decibels.

The client then encodes each recording into the game's own format,
FramelessQOA, on background threads. A minute of stereo takes about
100 ms on one core. The first time the game reads the sound it waits for
that encoding. A recording of one second or less (48000 frames) is held inline,
so its first play starts at once. A longer one streams from memory, as the
game's long sounds do. A server checks every file and holds nothing.

| Bound | |
| --- | --- |
| A file | at most 16 MiB on disk |
| A take | at most 10 minutes after its trim |
| A mod | at most 64 MiB of encoded recordings: about 46 minutes of mono, 23 of stereo |

A mod with sounds runs on both sides (`side = "both"`), so a client without
it cannot join. A file past a bound, of another format or outside the
mod's folder refuses the call. Wrapped in `load.feature("horn", ...)`, it
turns off that sound alone. The reason shows in the log and on the Mods
screen. A step can fail after the files were read. Then their recordings
stay served and count against the mod's bound. The id stays the mod's,
which may try it again.

```rust
use enshwick::content::id;
use enshwick::sound::{Channels, NewSound, NewTake};

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let mut steps = NewSound::new(id!("steps"), "steps/1.ogg");
    let mut take = NewTake::new("steps/2.flac");
    take.chance = Some(0.5);
    steps.takes = vec![NewTake::new("steps/1.ogg"), take];
    steps.channels = Channels::Mono;
    let _added = load.feature("steps", |load| load.add_sound(steps));
    Ok(())
}
```

```luau
local ok, added = pcall(enshwick.sound.add, {
    id = "you.steps:steps",
    takes = { { file = "steps/1.ogg" }, { file = "steps/2.flac", chance = 0.5 } },
    channels = "Mono",
})
```

A sound that misleads players in a fight, such as an attack that does not
happen, does not belong in a mod.

## Add a voiced line

With the `unstable` feature, `load.add_voiceover(new)` adds a voiced line.
No client has yet been heard playing one. It holds one sound per voice language, read as `add_sound` reads its files.
The game holds a slot per voice language. A language goes by the name of
its field of `Voiceover`, in snake case.

| Slot | Language |
| --- | --- |
| 1 | `english` |
| 2 | `german` |
| 3 | `french` |
| 4 | `spanish` |
| 5 | `simplified_chinese` |

The voice language setting picks the slot. Any other
setting speaks the first slot. An empty slot is silent, as most of the
game's French and Spanish lines are. A `fallback` language fills the slots
the line leaves empty.

Each language's container is `<id>.<language>`, such as
`you.lore:welcome.english`. The line records each language's length for
the game's lore queue. Dialogue lines, lore readings and journal entries
name a line by its GUID. Point one at yours through the
[resource values](resource-values.md): a dialogue line's `voiceovers`, a lore
reading's or a journal entry's `voiceover`.

`load.change_voiceover(line, new)` gives a game line new recordings in the
languages of `new` and keeps the others. Mods that change one line apply
in load order.

```rust
use enshwick::content::{Guid, id};
use enshwick::sound::{NewVoiceover, SoundChange, VoiceLine, VoiceoverId};

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let mut welcome = NewVoiceover::new(
            id!("welcome"),
            vec![VoiceLine::new("english", "voice/en/welcome.ogg"), VoiceLine::new("german", "voice/de/welcome.ogg")],
        );
    welcome.fallback = Some("english".into());
    let mut sound_change = SoundChange::default();
    sound_change.has_position = Some(false);
    welcome.container = sound_change;
    let _line = load.feature("welcome", |load| load.add_voiceover(welcome));
    let reading = VoiceoverId::from_guid(Guid::parse("3a7b1c90-5e2d-4f68-9c41-0d8e6b2f7a15"));
    let german = NewVoiceover::new(id!("reading"), vec![VoiceLine::new("german", "voice/de/reading.ogg")]);
    let _changed = load.feature("reading", |load| load.change_voiceover(reading, german));
    Ok(())
}
```

```luau
local ok, line = pcall(enshwick.sound.add_voiceover, {
    id = "you.lore:welcome",
    lines = { { language = "english", takes = { { file = "voice/en/welcome.ogg" } } } },
    fallback = "english",
})
```

## Add music to a place

With the `unstable` feature, `load.add_music(new)` adds audio files as
recordings and a music row of the client's ambience. The ambience plays
the row among the game's own music where and when it fits. It plays one
file at a time, and not the one it played last. No client has yet been
heard playing a mod's music row.

| `NewMusic` | Plays |
| --- | --- |
| `environment` | in one place of the game's `AmbientEnvironment`, such as Grassland or Cave; everywhere without one |
| `in_fog`, `indoors`, `in_battle` | only when it holds with `true`, only when not with `false`; both without one |
| `start_time`, `end_time` | from and to a time of day; a start after the end spans midnight |
| `volume` | from 0 to 1; 0.2, as most of the game's music away from battle |
| `sleep`, `sleep_randomness` | a pause after each file, at most 10 minutes |

Rows of several mods add up in load order. The returned recordings let
other containers play the files too.

```rust
use std::time::Duration;

use enshwick::content::id;
use enshwick::sound::NewMusic;

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let mut night = NewMusic::new(id!("night"), vec!["music/night1.ogg".into(), "music/night2.ogg".into()]);
    night.environment = Some("Grassland".into());
    night.in_battle = Some(false);
    night.start_time = Some(Duration::from_hours(22));
    night.end_time = Some(Duration::from_hours(6));
    night.sleep = Duration::from_secs(30);
    let _music = load.feature("night", |load| load.add_music(night));
    Ok(())
}
```

A boss's music is its template's `ecs::BossMusicTag`, whose `bossMusic`
names a container. Give a boss another one as any other
[template change](content/templates.md#what-a-change-does). Its container
plays in scene `Music`, such as one from `add_sound`. Only the client holds
this component, so the typed API has no type for it. The change names its
field as the game does:

```rust
use enshwick::content::Guid;
use enshwick::content::templates::ComponentChange;
use enshwick::resources::Value;
use enshwick::sound::SoundContainerId;

fn init(load: &mut enshwick::Load<'_>, boss: Guid, theme: SoundContainerId) -> Result<(), enshwick::Off> {
    let theme = theme.guid();
    let music = ComponentChange::edit_value("ecs::BossMusicTag", move |tag| {
        if let Some(container) = tag.field_mut("bossMusic") {
            *container = Value::Guid(theme);
        }
    });
    load.change_template(boss, vec![music])
}
```
