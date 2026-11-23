//! A mod's own sounds: recordings from audio files in its folder and a
//! container that picks among them.

use super::change::{
    self, ENTRIES, HAS_POSITION, MAX_DISTANCE_FIELD, MIN_DISTANCE, MODE, PITCH_FIELD, PRIORITY,
    SCENE, SPEAKER_VOLUME, VIBRATION_INTENSITY, VOLUME,
};
use super::decode::{Channels, Shape, Trim};
use super::{
    MAX_NEW_TAKES, SoundChange, SoundContainerId, SoundContainerScene, SoundId, Take, Takes,
    recording,
};
use crate::Off;
use crate::abi::Side;
use crate::content::{Id, types};
use crate::resources::Value;
use enshwick_content::__load::*;

pub use super::recording::{INLINE_FRAMES, MAX_FILE_BYTES, MAX_MOD_BYTES, SAMPLE_RATE};

const RANDOM: &str = "Random";
/// The content id part of each take's recording, in take order.
const TAKE_PARTS: [&str; MAX_NEW_TAKES] = [
    "sound", "sound2", "sound3", "sound4", "sound5", "sound6", "sound7", "sound8", "sound9",
    "sound10", "sound11", "sound12", "sound13", "sound14", "sound15", "sound16", "sound17",
    "sound18", "sound19", "sound20", "sound21", "sound22", "sound23", "sound24", "sound25",
    "sound26", "sound27", "sound28", "sound29", "sound30", "sound31", "sound32",
];

/// One take of a [`NewSound`]: an audio file in the mod's folder and its
/// weight among the sound's takes.
///
/// # Examples
///
/// See [`NewSound`].
#[derive(Clone, Debug, PartialEq, crate::Script)]
pub struct NewTake {
    /// The file, relative to the mod's folder: WAV (8, 16, 24 or 32-bit,
    /// or float), FLAC, Ogg Vorbis or MP3, at any rate, at most
    /// [`MAX_FILE_BYTES`].
    pub file: String,
    /// Its weight among the sound's takes, from 0 to 1; `None` for 1.
    #[script(default)]
    pub chance: Option<f32>,
}

impl NewTake {
    /// The take of `file` at weight 1.
    ///
    /// # Examples
    ///
    /// See [`NewSound`].
    #[must_use]
    pub fn new(file: impl Into<String>) -> Self {
        Self {
            file: file.into(),
            chance: None,
        }
    }

    fn chance(&self) -> f32 {
        self.chance.unwrap_or(1.0)
    }
}

/// A sound for [`Load::add_sound`](../struct.Load.html#method.add_sound):
/// audio files in the mod's folder, how they become recordings, and the
/// values of the sound container that picks among them.
///
/// # Examples
///
/// ```
/// use enshwick::content::id;
/// use enshwick::sound::{Channels, NewSound, NewTake, SoundChange, SoundContainerScene};
///
/// let steps = NewSound {
///     takes: vec![NewTake::new("steps/1.ogg"), NewTake::new("steps/2.ogg"), NewTake::new("steps/3.flac")],
///     channels: Channels::Mono,
///     gain_db: -3.0,
///     container: SoundChange { volume: Some(0.5), ..SoundChange::default() },
///     ..NewSound::new(id!("steps"), "steps/1.ogg")
/// };
/// assert_eq!(steps.takes.len(), 3);
/// let ui = NewSound {
///     container: SoundChange { scene: Some(SoundContainerScene::Ui), ..SoundChange::default() },
///     ..NewSound::new(id!("horn"), "sounds/horn.wav")
/// };
/// assert_eq!(ui.takes[0].file, "sounds/horn.wav");
/// ```
#[derive(Clone, Debug, crate::Script)]
pub struct NewSound {
    /// The sound's content id; its container's GUID is the id's.
    pub id: Id,
    /// Its takes, 1 to [`MAX_NEW_TAKES`]; the container picks one at random
    /// by their weights each time it plays.
    pub takes: Vec<NewTake>,
    /// Which channels its recordings keep; mono suits a sound placed in the
    /// world.
    #[script(default)]
    pub channels: Channels,
    /// A gain in decibels on every take, within
    /// [`GAIN_DB`](super::GAIN_DB).
    #[script(default)]
    pub gain_db: f32,
    /// The part of each take's file it plays.
    #[script(default)]
    pub trim: Trim,
    /// The container's values where they differ from those of a vanilla
    /// effect: scene [`Sfx`](SoundContainerScene::Sfx), volume and pitch 1,
    /// heard from 1 to 35 metres, priority 50. Its `takes` may only add to
    /// the sound's own.
    #[script(default)]
    pub container: SoundChange,
}

impl NewSound {
    /// The sound `id` of the one take `file`, an effect with the default
    /// values.
    ///
    /// # Examples
    ///
    /// See [`NewSound`].
    #[must_use]
    pub fn new(id: Id, file: impl Into<String>) -> Self {
        Self {
            id,
            takes: vec![NewTake::new(file)],
            channels: Channels::default(),
            gain_db: 0.0,
            trim: Trim::default(),
            container: SoundChange::default(),
        }
    }
}

/// What [`Load::add_sound`](../struct.Load.html#method.add_sound) made: the
/// container, which plays, and its recordings, which other containers'
/// [`Take`]s name.
///
/// # Examples
///
/// See [`Load::add_sound`](../struct.Load.html#method.add_sound).
#[derive(Clone, Debug, PartialEq, Eq, crate::Script)]
#[non_exhaustive]
pub struct AddedSound {
    /// The container.
    pub container: SoundContainerId,
    /// Its recordings, in the order of its takes.
    pub takes: Vec<SoundId>,
}

/// The value of a container that plays `own`, then the takes `change`
/// adds, with the values of a vanilla effect and `change`'s over them.
fn container_value(own: &[Take], entry_type: &str, change: &SoundChange) -> Result<Value, Off> {
    let fields = change.fields().map_err(|why| Off::new(why.to_string()))?;
    let float = |name: &str, value: f64| (name.to_owned(), Value::Float(value));
    let scene = change.scene.unwrap_or_default();
    let mut values = vec![
        (MODE.to_owned(), Value::Name(RANDOM.to_owned())),
        (SCENE.to_owned(), Value::Name(scene.name().to_owned())),
        float(VOLUME, 1.0),
        float(PITCH_FIELD, 1.0),
        (
            HAS_POSITION.to_owned(),
            Value::Bool(scene != SoundContainerScene::Ui),
        ),
        float(MIN_DISTANCE, 1.0),
        float(MAX_DISTANCE_FIELD, 35.0),
        float(SPEAKER_VOLUME, 1.0),
        float(VIBRATION_INTENSITY, 1.0),
        (PRIORITY.to_owned(), Value::Uint(50)),
    ];
    for (name, value) in fields {
        match values.iter_mut().find(|(held, _)| *held == name) {
            Some(slot) => slot.1 = value,
            None => values.push((name, value)),
        }
    }
    let mut takes = own.to_vec();
    match &change.takes {
        Some(Takes::Replace(_)) => {
            return Err(Off::new("a new sound's takes are its files: add to them"));
        }
        Some(Takes::Add(more)) => takes.extend_from_slice(more),
        None => {}
    }
    let entries = change::merged_entries(None, &Takes::Replace(takes), entry_type)
        .map_err(|why| Off::new(why.to_string()))?;
    values.push((ENTRIES.to_owned(), entries));
    Ok(Value::Struct(values))
}

/// `Load`'s methods of this module on the base handle, which core's
/// forwarders call.
#[doc(hidden)]
pub trait LoadMethods {
    /// As `Load::add_sound`.
    #[doc(hidden)]
    fn add_sound(&mut self, new: NewSound) -> Result<AddedSound, Off>;
}

impl LoadMethods for crate::BaseLoad {
    fn add_sound(&mut self, new: NewSound) -> Result<AddedSound, Off> {
        let NewSound {
            id,
            takes,
            channels,
            gain_db,
            trim,
            container: change,
        } = new;
        let fail = |why: Off| Off::new(format!("sound {id}: {why}"));
        if !self.joins() {
            return Err(fail(Off::new("a mod with sounds runs on both sides")));
        }
        if takes.is_empty() || takes.len() > MAX_NEW_TAKES {
            return Err(fail(Off::new(format!(
                "{} takes: a sound has 1 to {MAX_NEW_TAKES}",
                takes.len()
            ))));
        }
        change
            .fields()
            .map_err(|why| fail(Off::new(why.to_string())))?;
        let shape = Shape {
            channels,
            gain_db,
            trim,
        };
        let mut recordings = Vec::with_capacity(takes.len());
        let mut pending = 0;
        for take in &takes {
            if !(0.0..=1.0).contains(&take.chance()) {
                return Err(fail(Off::new(format!(
                    "{}: chance {} is outside 0 to 1",
                    take.file,
                    take.chance()
                ))));
            }
            let recording = recording::read(self, &take.file, shape, pending).map_err(fail)?;
            pending += recording.bytes();
            recordings.push(recording);
        }
        let client = self.host().build().side == Side::CLIENT;
        let host = self.host();
        let entry_type = if client {
            for take in change::named_sounds(&change) {
                super::known_sound(host, SoundId::from_guid(take)).map_err(fail)?;
            }
            let entry_type = super::entry_type(self).map_err(fail)?;
            // Found before any id is declared, so a game without them leaves none.
            recording::bases(self).map_err(fail)?;
            Some(entry_type)
        } else {
            None
        };
        let container = self.declare(id)?;
        let mut own = Vec::with_capacity(takes.len());
        for ((take, recording), part) in takes.iter().zip(recordings).zip(TAKE_PARTS) {
            let sound_id = id
                .try_part(part)
                .ok_or_else(|| fail(Off::new("the id has no room for its takes' parts")))?;
            let sound = SoundId::from_guid(self.declare(sound_id)?);
            recording::serve(self, sound, recording, false, sound_id.to_string()).map_err(fail)?;
            own.push(Take {
                chance: take.chance(),
                ..Take::new(sound)
            });
        }
        if let Some(entry_type) = entry_type {
            let value = container_value(&own, &entry_type, &change).map_err(fail)?;
            crate::resources::serve_new(
                self.host(),
                types::SOUND_CONTAINER_RESOURCE.key(container)?,
                &value,
            )?;
        }
        Ok(AddedSound {
            container: SoundContainerId::from_guid(container),
            takes: own.into_iter().map(|take| take.sound).collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::Guid;

    #[test]
    fn a_container_plays_its_takes_with_the_given_values_over_the_defaults() {
        let sound = SoundId::from_guid(Guid::named("you.horns:horn.sound"));
        let other = SoundId::from_guid(Guid::named("you.horns:horn.sound2"));
        let own = [
            Take::new(sound),
            Take {
                chance: 0.25,
                ..Take::new(other)
            },
        ];
        let change = SoundChange {
            scene: Some(SoundContainerScene::Ui),
            volume: Some(0.5),
            ..SoundChange::default()
        };
        let value = container_value(&own, "demo::Entry", &change).unwrap();
        assert_eq!(value.field(VOLUME), Some(&Value::Float(0.5)));
        assert_eq!(value.field(SCENE), Some(&Value::Name("Ui".into())));
        assert_eq!(value.field(HAS_POSITION), Some(&Value::Bool(false)));
        let effect = container_value(&own, "demo::Entry", &SoundChange::default()).unwrap();
        assert_eq!(effect.field(HAS_POSITION), Some(&Value::Bool(true)));
        assert_eq!(
            value.field(ENTRIES),
            Some(&Value::List(
                own.iter()
                    .map(|t| change::entry("demo::Entry", *t))
                    .collect()
            ))
        );
    }

    #[test]
    fn a_new_container_adds_given_takes_after_its_own_and_refuses_to_replace_them() {
        let sound = SoundId::from_guid(Guid([1; 16]));
        let other = SoundId::from_guid(Guid([2; 16]));
        let more = SoundChange {
            takes: Some(Takes::Add(vec![Take::new(other)])),
            ..SoundChange::default()
        };
        let value = container_value(&[Take::new(sound)], "demo::Entry", &more).unwrap();
        assert_eq!(
            value.field(ENTRIES),
            Some(&Value::List(vec![
                change::entry("demo::Entry", Take::new(sound)),
                change::entry("demo::Entry", Take::new(other)),
            ]))
        );
        let replaced = SoundChange {
            takes: Some(Takes::Replace(vec![Take::new(other)])),
            ..SoundChange::default()
        };
        assert!(container_value(&[Take::new(sound)], "demo::Entry", &replaced).is_err());
    }

    #[test]
    fn every_take_has_a_part_that_fits_an_id() {
        let id = Id::new("you.horns:horn");
        let parts: Vec<_> = TAKE_PARTS.iter().map(|part| id.try_part(part)).collect();
        assert!(parts.iter().all(Option::is_some));
        let mut guids: Vec<_> = parts.iter().flatten().map(|id| id.guid()).collect();
        guids.dedup();
        assert_eq!(guids.len(), MAX_NEW_TAKES);
    }
}
