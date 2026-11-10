<!--
type: how-to
description: A mod plays the game's sound containers on a client, changes their volume, pitch and distances, and adds its own sounds from WAV files in its folder.
-->

# Sounds

A sound container is what the game plays for one event: a set of sounds
and how it plays them. A mod names one by its GUID, a
[`SoundContainerId`](https://github.com/enshwick/enshwick/blob/main/crates/internal/core/src/sound.rs),
and plays it, changes it or adds its own.

## Play a container on a client

Inside a frame of the interface, `frame.play(id)` plays a container as an
interface sound. Only that client hears it, and it comes from no place in
the world. The [interface chapter](interface.md) lists it with the widgets.

## Change a vanilla sound

`load.change_sound(id, change)` sets the values a
[`SoundChange`](https://github.com/enshwick/enshwick/blob/main/crates/internal/core/src/sound.rs)
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
    load.serve(key, key, move |base| {
        let mut notifier = registry.decode(NOTIFIER, base).ok()?;
        *notifier.field_mut("defaultSound")? = Value::Guid(bell.guid());
        registry.encode(NOTIFIER, &notifier, Some(base)).ok()
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
