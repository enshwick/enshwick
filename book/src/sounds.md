<!--
type: how-to
description: A mod plays the game's sound containers on a client, changes their volume, pitch and distances, and adds its own sounds from WAV files in its folder.
-->

# Sounds

A sound container is what the game plays for one event: a set of sounds
and how it plays them. A mod names one by its GUID, a
[`SoundContainerId`](https://github.com/enshwick/enshwick/blob/main/crates/internal/ui/src/sound.rs),
and plays it, changes it or adds its own.

## Play a container on a client

Inside a frame of the interface, `frame.play(id)` plays a container as an
interface sound. Only that client hears it, and it comes from no place in
the world. The [interface chapter](interface.md) lists it with the widgets.

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
| `looped` | true or false |
| `priority` | 0 to 65535 |

```rust
use enshwick::content::Guid;
use enshwick::sound::{SoundChange, SoundContainerId};

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let bell = SoundContainerId::from_guid(Guid::named("you.bells:bell"));
    load.change_sound(bell, SoundChange { volume: Some(0.2), ..SoundChange::default() })
}
```

```luau
enshwick.sound.change("<container GUID>", { volume = 0.2 })
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
(`MAX_NOTIFIES_PER_STEP`). The call past that bound is refused. A notifier
that misleads players in a fight does not belong in a mod.

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
enshwick.effects.notify({
    notifier = "<notifier GUID>",
    at = { Position = vector.create(0, 100, 0) },
    sound_volume_hint = "FriendlyNpc",
})
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

With the `unstable` feature, `load.add_sound(NewSound::new(id!("horn"),
"sounds/horn.wav"))` adds that WAV file of the mod's folder and a
container that plays it. It returns the container. Its GUID is the
[content id](concepts/ids.md)'s, so other mods play it by `you.horns:horn`. A data pack lists
its sounds in [`sounds.toml`](content/packs.md#soundstoml).

The client holds the sound as the game holds its own uncompressed sounds,
streamed from memory. A server checks the file and holds nothing.

| A WAV file | |
| --- | --- |
| Samples | 16-bit PCM |
| Channels | mono or stereo |
| Rate | 48000 Hz, the rate of every sound the game plays |
| Size | at most 16 MiB a file and 64 MiB a mod |
| Length | what 16 MiB holds: about 87 seconds of stereo, 174 of mono |

A mod with sounds runs on both sides (`side = "both"`), so a client without
it cannot join. A file past a bound, of another format or outside the
mod's folder refuses the call. Wrapped in `load.feature("horn", ...)`, it
turns off that sound alone. The reason shows in the log and on the Mods
screen.

A sound that misleads players in a fight, such as an attack that does not
happen, does not belong in a mod.
