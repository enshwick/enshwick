//! A mod's own sounds: a recording from a file in its folder and a
//! container that plays it.

use super::change::{
    self, ENTRIES, HAS_POSITION, MAX_DISTANCE_FIELD, MIN_DISTANCE, MODE, PITCH_FIELD, PRIORITY,
    SCENE, SPEAKER_VOLUME, VIBRATION_INTENSITY, VOLUME,
};
use super::{SoundChange, SoundContainerId, SoundContainerScene, SoundId, Take, recording};
use crate::Off;
use crate::abi::Side;
use crate::content::{Id, types};
use crate::resources::Value;
use enshwick_content::__load::*;

pub use super::recording::{MAX_FILE_BYTES, MAX_MOD_BYTES, SAMPLE_RATE};

const RANDOM: &str = "Random";
const SOUND_PART: &str = "sound";

/// A sound for [`Load::add_sound`](../struct.Load.html#method.add_sound): a
/// WAV file in the mod's folder and the values of the sound container that
/// plays it.
///
/// # Examples
///
/// ```
/// use enshwick::content::id;
/// use enshwick::sound::{NewSound, SoundChange, SoundContainerScene};
///
/// let horn = NewSound {
///     container: SoundChange {
///         scene: Some(SoundContainerScene::Ui),
///         volume: Some(0.5),
///         ..SoundChange::default()
///     },
///     ..NewSound::new(id!("horn"), "sounds/horn.wav")
/// };
/// assert_eq!(horn.file, "sounds/horn.wav");
/// ```
#[derive(Clone, Debug, crate::Script)]
pub struct NewSound {
    /// The sound's content id; its container's GUID is the id's.
    pub id: Id,
    /// The WAV file, relative to the mod's folder: 16-bit PCM, mono or
    /// stereo, at [`SAMPLE_RATE`], at most [`MAX_FILE_BYTES`].
    pub file: String,
    /// The container's values where they differ from those of a vanilla
    /// effect: scene [`Sfx`](SoundContainerScene::Sfx), volume and pitch 1,
    /// heard from 1 to 35 metres, priority 50. Its takes follow the
    /// recording.
    #[script(default)]
    pub container: SoundChange,
}

impl NewSound {
    /// The sound `id` from `file`, an effect with the default values.
    ///
    /// # Examples
    ///
    /// See [`NewSound`].
    #[must_use]
    pub fn new(id: Id, file: impl Into<String>) -> Self {
        Self {
            id,
            file: file.into(),
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

/// The value of a container that plays `sound` with the values of a vanilla
/// effect, `fields` written over them.
fn container_value(sound: SoundId, entry_type: &str, change: &SoundChange) -> Result<Value, Off> {
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
    let own = super::Takes::Replace(vec![Take::new(sound)]);
    let takes = match &change.takes {
        Some(super::Takes::Replace(_)) => {
            return Err(Off::new(
                "a new sound's takes follow its recordings: add to them",
            ));
        }
        Some(super::Takes::Add(more)) => {
            let mut all = vec![Take::new(sound)];
            all.extend_from_slice(more);
            super::Takes::Replace(all)
        }
        None => own,
    };
    let entries = change::merged_entries(None, &takes, entry_type)
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
            file,
            container: change,
        } = new;
        let fail = |why: Off| Off::new(format!("sound {id}: {why}"));
        if !self.joins() {
            return Err(fail(Off::new("a mod with sounds runs on both sides")));
        }
        change
            .fields()
            .map_err(|why| fail(Off::new(why.to_string())))?;
        let sound_id = id
            .try_part(SOUND_PART)
            .ok_or_else(|| fail(Off::new("the id has no room for its sound's part")))?;
        let recording = recording::read(self, &file).map_err(fail)?;
        let client = self.host().build().side == Side::CLIENT;
        let host = self.host();
        let entry_type = if client {
            for take in change::named_sounds(&change) {
                super::known_sound(host, SoundId::from_guid(take)).map_err(fail)?;
            }
            let entry_type = super::entry_type(self).map_err(fail)?;
            // Found before any id is declared, so a game without one leaves none.
            recording::base(self).map_err(fail)?;
            Some(entry_type)
        } else {
            None
        };
        let container = self.declare(id)?;
        let sound = SoundId::from_guid(self.declare(sound_id)?);
        recording::serve(self, sound, recording, false, id.to_string()).map_err(fail)?;
        if let Some(entry_type) = entry_type {
            let value = container_value(sound, &entry_type, &change).map_err(fail)?;
            crate::resources::serve_new(
                self.host(),
                types::SOUND_CONTAINER_RESOURCE.key(container)?,
                &value,
            )?;
        }
        Ok(AddedSound {
            container: SoundContainerId::from_guid(container),
            takes: vec![sound],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::Guid;

    #[test]
    fn a_container_plays_its_one_sound_with_the_given_values_over_the_defaults() {
        let sound = SoundId::from_guid(Guid::named("you.horns:horn.sound"));
        let change = SoundChange {
            scene: Some(SoundContainerScene::Ui),
            volume: Some(0.5),
            ..SoundChange::default()
        };
        let value = container_value(sound, "demo::Entry", &change).unwrap();
        assert_eq!(value.field(VOLUME), Some(&Value::Float(0.5)));
        assert_eq!(value.field(SCENE), Some(&Value::Name("Ui".into())));
        assert_eq!(value.field(HAS_POSITION), Some(&Value::Bool(false)));
        let effect = container_value(sound, "demo::Entry", &SoundChange::default()).unwrap();
        assert_eq!(effect.field(HAS_POSITION), Some(&Value::Bool(true)));
        let Some(Value::List(entries)) = value.field(ENTRIES) else {
            panic!("no entries");
        };
        let [Value::Variant(entry)] = entries.as_slice() else {
            panic!("one entry");
        };
        assert_eq!(entry.type_name(), "demo::Entry");
        assert_eq!(
            entry.value().field(change::ENTRY_SOUND),
            Some(&Value::Guid(sound.guid()))
        );
    }

    #[test]
    fn a_new_container_adds_given_takes_after_its_recording_and_refuses_to_replace_it() {
        let sound = SoundId::from_guid(Guid([1; 16]));
        let other = SoundId::from_guid(Guid([2; 16]));
        let more = SoundChange {
            takes: Some(super::super::Takes::Add(vec![Take::new(other)])),
            ..SoundChange::default()
        };
        let value = container_value(sound, "demo::Entry", &more).unwrap();
        let Some(Value::List(entries)) = value.field(ENTRIES) else {
            panic!("no entries");
        };
        assert_eq!(
            entries,
            &vec![
                change::entry("demo::Entry", Take::new(sound)),
                change::entry("demo::Entry", Take::new(other)),
            ]
        );
        let replaced = SoundChange {
            takes: Some(super::super::Takes::Replace(vec![Take::new(other)])),
            ..SoundChange::default()
        };
        assert!(container_value(sound, "demo::Entry", &replaced).is_err());
    }
}
