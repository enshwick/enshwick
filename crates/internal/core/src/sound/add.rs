//! A mod's own sounds: a WAV file in its folder, served as the game serves
//! its uncompressed sounds, and a container that plays it.

use super::{
    MAX_DISTANCE_FIELD, MIN_DISTANCE, PITCH_FIELD, PRIORITY, SoundChange, SoundContainerId, VOLUME,
};
use crate::abi::Side;
use crate::content::{EngineFacts, Guid, Id, types};
use crate::resources::{Registry, Value, Variant};
use crate::{Load, Off};

/// The rate of every sound the game plays, and of a WAV file a mod adds,
/// in frames per second.
pub const SAMPLE_RATE: u32 = 48_000;
/// The largest WAV file a mod adds as a sound, in bytes.
pub const MAX_FILE_BYTES: u64 = 16 << 20;
/// The most bytes of WAV files one mod adds as sounds.
pub const MAX_MOD_BYTES: u64 = 64 << 20;

const FORMAT: &str = "format";
const UNCOMPRESSED: &str = "Uncompressed";
const CHANNELS: &str = "channelConfiguration";
const MONO: &str = "Mono";
const STEREO: &str = "Stereo";
const FRAME_COUNT: &str = "frameCount";
const DURATION: &str = "duration";
const TIME_VALUE: &str = "value";
const STREAMED: &str = "streamedData";
const CONTENT_HASH: &str = "hash";
const CONTENT_HASH_PARTS: [&str; 4] = ["size", "hash0", "hash1", "hash2"];
const DEBUG_NAME: &str = "debugName";
const MODE: &str = "mode";
const RANDOM: &str = "Random";
const SCENE: &str = "scene";
const HAS_POSITION: &str = "hasPosition";
const SPEAKER_VOLUME: &str = "gamepadSpeakerVolume";
const VIBRATION: &str = "gamepadVibrationIntensity";
const ENTRIES: &str = "entries";
const ENTRY_CHANCE: &str = "chance";
const ENTRY_SOUND: &str = "sound";

/// The game's scene of a sound container, `SoundContainerScene`.
///
/// # Examples
///
/// See [`NewSound`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, crate::Script)]
#[non_exhaustive]
pub enum SoundContainerScene {
    /// A sound effect.
    #[default]
    Sfx,
    /// Music.
    Music,
    /// An interface sound, heard from no place in the world.
    Ui,
    /// A voice.
    Voice,
}

impl SoundContainerScene {
    const fn name(self) -> &'static str {
        match self {
            Self::Sfx => "Sfx",
            Self::Music => "Music",
            Self::Ui => "Ui",
            Self::Voice => "Voice",
        }
    }
}

/// A sound for [`Load::add_sound`]: a WAV file in the mod's folder and the
/// values of the sound container that plays it.
///
/// # Examples
///
/// ```
/// # extern crate enshwick_core as enshwick;
/// use enshwick::content::id;
/// use enshwick::sound::{NewSound, SoundChange, SoundContainerScene};
///
/// let horn = NewSound {
///     scene: SoundContainerScene::Ui,
///     container: SoundChange { volume: Some(0.5), ..SoundChange::default() },
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
    /// The container's scene.
    #[script(default)]
    pub scene: SoundContainerScene,
    /// The container's values where they differ from those of a vanilla
    /// effect: volume and pitch 1, heard from 1 to 35 metres, priority 50.
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
            scene: SoundContainerScene::default(),
            container: SoundChange::default(),
        }
    }
}

/// Why a WAV file is not a sound a mod adds.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub(crate) enum ReadWavError {
    /// The bytes are no WAV file the reader takes.
    #[error("not a WAV file: {0}")]
    Malformed(String),
    /// The samples are of another format.
    #[error(
        "{bits}-bit samples in {channels} channels at {rate} Hz: a sound is 16-bit PCM, \
         mono or stereo, at {SAMPLE_RATE} Hz"
    )]
    Format {
        /// Bits per sample.
        bits: u16,
        /// Channels.
        channels: u16,
        /// Frames per second.
        rate: u32,
    },
    /// The file holds no samples.
    #[error("no samples")]
    Empty,
}

/// What a mod's sounds hold while it initialises: the bytes of its files so
/// far, and the game's streamed uncompressed sound they build from, once
/// one was found.
#[derive(Clone, Copy, Debug, Default)]
struct Budget {
    bytes: u64,
    base: Option<crate::abi::ResourceKey>,
}

/// A sound's samples as the game stores an uncompressed one: 16-bit
/// little-endian, the channels of each frame side by side.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Pcm {
    pub(crate) stereo: bool,
    pub(crate) frames: u32,
    pub(crate) bytes: Vec<u8>,
}

/// Reads `wav` as a sound a mod adds.
pub(crate) fn read_wav(wav: &[u8]) -> Result<Pcm, ReadWavError> {
    let reader = hound::WavReader::new(std::io::Cursor::new(wav))
        .map_err(|error| ReadWavError::Malformed(error.to_string()))?;
    let spec = reader.spec();
    if spec.sample_format != hound::SampleFormat::Int
        || spec.bits_per_sample != 16
        || !matches!(spec.channels, 1 | 2)
        || spec.sample_rate != SAMPLE_RATE
    {
        return Err(ReadWavError::Format {
            bits: spec.bits_per_sample,
            channels: spec.channels,
            rate: spec.sample_rate,
        });
    }
    let frames = reader.duration();
    if frames == 0 {
        return Err(ReadWavError::Empty);
    }
    // The header's length is a claim: past the file's bytes it would size
    // the buffer, and the game would read the sound past its end.
    if u64::from(reader.len()) * 2 > u64::try_from(wav.len()).unwrap_or(u64::MAX) {
        return Err(ReadWavError::Malformed(
            "its header claims more samples than the file holds".to_owned(),
        ));
    }
    let mut bytes = Vec::with_capacity(usize::try_from(reader.len()).unwrap_or(0) * 2);
    for sample in reader.into_samples::<i16>() {
        let sample = sample.map_err(|error| ReadWavError::Malformed(error.to_string()))?;
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    Ok(Pcm {
        stereo: spec.channels == 2,
        frames,
        bytes,
    })
}

/// A path that leaves the mod's folder.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{0:?} is no path inside the mod's folder")]
pub(crate) struct OutsideFolderError(String);

/// The file `file` names inside `dir`: a relative path that stays inside it.
fn file_in(dir: &std::path::Path, file: &str) -> Result<std::path::PathBuf, OutsideFolderError> {
    if file.is_empty() || !crate::mods::stays_inside(file) {
        return Err(OutsideFolderError(file.to_owned()));
    }
    Ok(dir.join(file))
}

/// The value of a container that plays `sound` in `scene` with the values
/// of a vanilla effect, `fields` written over them.
fn container_value(
    sound: Guid,
    scene: SoundContainerScene,
    entry_type: &str,
    fields: Vec<(String, Value)>,
) -> Value {
    let float = |name: &str, value: f64| (name.to_owned(), Value::Float(value));
    let entry = Value::Struct(vec![
        float(ENTRY_CHANCE, 1.0),
        (ENTRY_SOUND.to_owned(), Value::Guid(sound)),
    ]);
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
        float(VIBRATION, 1.0),
        (PRIORITY.to_owned(), Value::Uint(50)),
        (
            ENTRIES.to_owned(),
            Value::List(vec![Value::Variant(Box::new(Variant::new(
                entry_type.to_owned(),
                entry,
            )))]),
        ),
    ];
    for (name, value) in fields {
        match values.iter_mut().find(|(held, _)| *held == name) {
            Some(slot) => slot.1 = value,
            None => values.push((name, value)),
        }
    }
    Value::Struct(values)
}

/// `base`, a vanilla uncompressed sound streamed as content, made to hold
/// `frames` frames served as content `hash` under `name`; [`Off`] when the
/// base lacks a field the frames need, so a renamed field serves nothing
/// rather than the base's layout over the mod's frames.
fn sound_value(
    mut base: Value,
    stereo: bool,
    frames: u32,
    hash: [u32; 4],
    name: &str,
) -> Result<Value, Off> {
    let nanos = i64::try_from(u64::from(frames) * 1_000_000_000 / u64::from(SAMPLE_RATE))
        .unwrap_or(i64::MAX);
    let mut set = |field: &str, value: Value| {
        let slot = base
            .field_mut(field)
            .ok_or_else(|| Off::new(format!("the game's sound has no {field}")))?;
        *slot = value;
        Ok::<_, Off>(())
    };
    set(
        CHANNELS,
        Value::Name(if stereo { STEREO } else { MONO }.to_owned()),
    )?;
    set(FRAME_COUNT, Value::Uint(u64::from(frames)))?;
    set(
        DURATION,
        Value::Struct(vec![(TIME_VALUE.to_owned(), Value::Int(nanos))]),
    )?;
    let _named = set(DEBUG_NAME, Value::Text(name.to_owned()));
    let content = base
        .field_mut(STREAMED)
        .and_then(|streamed| streamed.field_mut(CONTENT_HASH))
        .ok_or_else(|| Off::new(format!("the game's sound has no {STREAMED}.{CONTENT_HASH}")))?;
    *content = Value::Struct(
        CONTENT_HASH_PARTS
            .iter()
            .zip(hash)
            .map(|(part, word)| ((*part).to_owned(), Value::Uint(u64::from(word))))
            .collect(),
    );
    Ok(base)
}

/// Whether `value` is an uncompressed sound whose samples stream as content.
fn is_streamed_uncompressed(value: &Value) -> bool {
    let size = value
        .field(STREAMED)
        .and_then(|streamed| streamed.field(CONTENT_HASH))
        .and_then(|hash| hash.field(CONTENT_HASH_PARTS[0]));
    value.field(FORMAT) == Some(&Value::Name(UNCOMPRESSED.to_owned()))
        && matches!(size, Some(Value::Uint(size)) if *size > 0)
}

#[crate::script(sound_add)]
impl Load<'_> {
    /// Unstable: needs the `unstable` feature and may change in any release.
    ///
    /// Adds the sound in `new.file` and a container that plays it, under
    /// `new.id`, and returns the container, which
    /// [`Frame::play`](crate::ui::Frame::play), [`Load::change_sound`] and
    /// other mods use. The client holds it as the game holds its own
    /// uncompressed sounds, built from one of them, streamed from memory;
    /// nothing is written to the archive or a save. A server checks the
    /// file and serves nothing. Wrap the call in [`Load::feature`] so a bad
    /// file turns off that sound alone, with the reason on the Mods screen.
    ///
    /// # Errors
    ///
    /// [`Off`] for a mod not of side both, a path outside the mod's folder,
    /// a file past [`MAX_FILE_BYTES`] or past the mod's [`MAX_MOD_BYTES`],
    /// one that is not 16-bit PCM WAV at [`SAMPLE_RATE`], a container value
    /// outside its bound, an id another
    /// content id holds, after init, and on a client whose game holds no
    /// streamed uncompressed sound to build from.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_core as enshwick;
    /// use enshwick::content::id;
    /// use enshwick::sound::NewSound;
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let horn = load.feature("horn", |load| load.add_sound(NewSound::new(id!("horn"), "sounds/horn.wav")));
    ///     load.on_frame(move |frame| {
    ///         if let Some(horn) = horn
    ///             && frame.button("Horn")
    ///         {
    ///             let _played = frame.play(horn);
    ///         }
    ///     })
    /// }
    /// ```
    #[script(name = "add")]
    pub fn add_sound(&mut self, new: NewSound) -> Result<SoundContainerId, Off> {
        let NewSound {
            id,
            file,
            scene,
            container: change,
        } = new;
        let fail = |why: String| Off::new(format!("sound {id}: {why}"));
        if !self.joins() {
            return Err(fail("a mod with sounds runs on both sides".to_owned()));
        }
        let fields = change.fields().map_err(|why| fail(why.to_string()))?;
        let sound_id = id
            .try_part(ENTRY_SOUND)
            .ok_or_else(|| fail("the id has no room for its sound's part".to_owned()))?;
        let path = file_in(&self.mod_dir()?, &file).map_err(|why| fail(why.to_string()))?;
        let size = std::fs::metadata(&path)
            .map_err(|error| fail(format!("{file}: {error}")))?
            .len();
        if size > MAX_FILE_BYTES {
            return Err(fail(format!("{file}: {size} bytes, past {MAX_FILE_BYTES}")));
        }
        if self.slot::<Budget>().unwrap_or_default().bytes + size > MAX_MOD_BYTES {
            return Err(fail(format!(
                "{file}: the mod's sounds would pass {MAX_MOD_BYTES} bytes"
            )));
        }
        let wav = std::fs::read(&path).map_err(|error| fail(format!("{file}: {error}")))?;
        let pcm = read_wav(&wav).map_err(|error| fail(format!("{file}: {error}")))?;
        drop(wav);
        if self.host().build().side != Side::CLIENT {
            let container = self.declare(id)?;
            self.declare(sound_id)?;
            self.slot::<Budget>().get_or_insert_default().bytes += size;
            return Ok(SoundContainerId::from_guid(container));
        }
        let host = self.host();
        let registry = Registry::game()?;
        let sound_type = registry
            .hashed(types::SOUND_RESOURCE.hash()?)
            .ok_or_else(|| fail("this game has no sound type".to_owned()))?;
        let entry_type = registry
            .hashed(self.hash(crate::engine_ids::SOUND_CONTAINER_SOUND_ENTRY)?)
            .ok_or_else(|| fail("this game has no sound container entry".to_owned()))?
            .qualified_name()
            .to_owned();
        let base = match self.slot::<Budget>().unwrap_or_default().base {
            Some(base) => base,
            // One of the game's few uncompressed sounds, all streamed, carries
            // the content category a served sound needs.
            None => host
                .resource_keys(Some(sound_type))?
                .into_iter()
                .find(|key| {
                    host.game_value(*key)
                        .is_ok_and(|value| is_streamed_uncompressed(&value))
                })
                .ok_or_else(|| {
                    fail("the game holds no streamed uncompressed sound to build from".to_owned())
                })?,
        };
        self.slot::<Budget>().get_or_insert_default().base = Some(base);
        let container = self.declare(id)?;
        let sound = self.declare(sound_id)?;
        let host = self.host();
        let hash = host.serve_content(&pcm.bytes)?;
        // Served bytes stay held whatever fails after them.
        self.slot::<Budget>().get_or_insert_default().bytes += size;
        let (stereo, frames, name) = (pcm.stereo, pcm.frames, id.to_string());
        drop(pcm);
        crate::resources::serve_value(
            host,
            types::SOUND_RESOURCE.key(sound)?,
            base,
            move |value| sound_value(value, stereo, frames, hash, &name),
        )?;
        let value = container_value(sound, scene, &entry_type, fields);
        crate::resources::serve_new(
            host,
            types::SOUND_CONTAINER_RESOURCE.key(container)?,
            &value,
        )?;
        Ok(SoundContainerId::from_guid(container))
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn wav(channels: u16, rate: u32, bits: u16, samples: &[i16]) -> Vec<u8> {
        let spec = hound::WavSpec {
            channels,
            sample_rate: rate,
            bits_per_sample: bits,
            sample_format: hound::SampleFormat::Int,
        };
        let mut out = std::io::Cursor::new(Vec::new());
        let mut writer = hound::WavWriter::new(&mut out, spec).unwrap();
        for &sample in samples {
            if bits == 16 {
                writer.write_sample(sample).unwrap();
            } else {
                writer.write_sample(i32::from(sample)).unwrap();
            }
        }
        writer.finalize().unwrap();
        out.into_inner()
    }

    #[test]
    fn a_wav_of_another_format_is_refused_naming_it() {
        for (channels, rate, bits) in [(1, 44_100, 16), (3, SAMPLE_RATE, 16), (1, SAMPLE_RATE, 24)]
        {
            assert_eq!(
                read_wav(&wav(channels, rate, bits, &[1, 2, 3])),
                Err(ReadWavError::Format {
                    bits,
                    channels,
                    rate
                })
            );
        }
        assert_eq!(
            read_wav(&wav(1, SAMPLE_RATE, 16, &[])),
            Err(ReadWavError::Empty)
        );
        assert!(matches!(read_wav(b"RIFF"), Err(ReadWavError::Malformed(_))));
    }

    #[test]
    fn a_header_that_claims_more_samples_than_the_file_holds_is_refused() {
        let mut lying = wav(1, SAMPLE_RATE, 16, &[1, 2, 3]);
        let data = lying.len() - 6 - 4;
        // Whole samples, so only the length check refuses it.
        lying[data..data + 4].copy_from_slice(&0x7fff_fffe_u32.to_le_bytes());
        assert!(
            matches!(&read_wav(&lying), Err(ReadWavError::Malformed(why)) if why.contains("claims more samples")),
            "{:?}",
            read_wav(&lying)
        );
        // Exactly as many samples as the file holds pass.
        let whole = wav(2, SAMPLE_RATE, 16, &[1, 2, 3, 4]);
        assert_eq!(read_wav(&whole).map(|p| p.frames), Ok(2));
        assert_eq!(
            read_wav(&wav(1, SAMPLE_RATE, 16, &[1, 2, 3])).map(|p| p.frames),
            Ok(3)
        );
    }

    #[test]
    fn a_path_must_stay_inside_the_mod_s_folder() {
        let dir = std::path::Path::new("mods/you.horns");
        assert_eq!(
            file_in(dir, "sounds/horn.wav"),
            Ok(dir.join("sounds/horn.wav"))
        );
        assert_eq!(file_in(dir, "./horn.wav"), Ok(dir.join("./horn.wav")));
        for bad in [
            "",
            "../other/horn.wav",
            "/horn.wav",
            "sounds/../../horn.wav",
        ] {
            assert!(file_in(dir, bad).is_err(), "{bad}");
        }
        // Elsewhere a drive letter is an ordinary folder name inside the mod's.
        if cfg!(windows) {
            assert!(file_in(dir, "C:/horn.wav").is_err());
        }
    }

    #[test]
    fn a_new_sound_takes_its_frames_channels_and_content_over_the_base() {
        let base = Value::Struct(vec![
            (FORMAT.into(), Value::Name(UNCOMPRESSED.into())),
            (CHANNELS.into(), Value::Name(MONO.into())),
            (FRAME_COUNT.into(), Value::Uint(1)),
            (
                DURATION.into(),
                Value::Struct(vec![(TIME_VALUE.into(), Value::Int(1))]),
            ),
            (
                STREAMED.into(),
                Value::Struct(vec![
                    (
                        CONTENT_HASH.into(),
                        Value::Struct(vec![(CONTENT_HASH_PARTS[0].into(), Value::Uint(2))]),
                    ),
                    ("category".into(), Value::Uint(9)),
                ]),
            ),
            (DEBUG_NAME.into(), Value::Text("waterfall".into())),
        ]);
        assert!(is_streamed_uncompressed(&base));
        let over_base = |base: Value| {
            sound_value(
                base,
                true,
                SAMPLE_RATE * 2,
                [192_000, 1, 2, 3],
                "you.horns:horn",
            )
        };
        let Value::Struct(fields) = &base else {
            unreachable!("built as a struct")
        };
        let renamed = fields
            .iter()
            .map(|(name, value)| {
                let name = if name == FRAME_COUNT {
                    "frames"
                } else {
                    name.as_str()
                };
                (name.to_owned(), value.clone())
            })
            .collect();
        assert!(
            over_base(Value::Struct(renamed)).is_err(),
            "a base without a field the frames need serves nothing"
        );
        let built = over_base(base).unwrap();
        assert!(is_streamed_uncompressed(&built));
        assert_eq!(built.field(CHANNELS), Some(&Value::Name(STEREO.into())));
        assert_eq!(
            built.field(DURATION).and_then(|d| d.field(TIME_VALUE)),
            Some(&Value::Int(2_000_000_000))
        );
        let streamed = built.field(STREAMED).unwrap();
        assert_eq!(streamed.field("category"), Some(&Value::Uint(9)), "kept");
        assert_eq!(
            streamed
                .field(CONTENT_HASH)
                .and_then(|h| h.field(CONTENT_HASH_PARTS[3])),
            Some(&Value::Uint(3))
        );
    }

    #[test]
    fn a_container_plays_its_one_sound_with_the_given_values_over_the_defaults() {
        let sound = Guid::named("you.horns:horn.sound");
        let fields = vec![(VOLUME.to_owned(), Value::Float(0.5))];
        let value = container_value(sound, SoundContainerScene::Ui, "demo::Entry", fields);
        assert_eq!(value.field(VOLUME), Some(&Value::Float(0.5)));
        assert_eq!(value.field(SCENE), Some(&Value::Name("Ui".into())));
        assert_eq!(value.field(HAS_POSITION), Some(&Value::Bool(false)));
        let effect = container_value(sound, SoundContainerScene::Sfx, "demo::Entry", Vec::new());
        assert_eq!(effect.field(HAS_POSITION), Some(&Value::Bool(true)));
        let Some(Value::List(entries)) = value.field(ENTRIES) else {
            panic!("no entries");
        };
        let [Value::Variant(entry)] = entries.as_slice() else {
            panic!("one entry");
        };
        assert_eq!(entry.type_name(), "demo::Entry");
        assert_eq!(entry.value().field(ENTRY_SOUND), Some(&Value::Guid(sound)));
    }

    proptest! {
        #[test]
        fn every_16_bit_wav_at_the_game_s_rate_reads_back_its_samples(
            stereo in any::<bool>(),
            frames in prop::collection::vec(any::<i16>(), 1..512),
        ) {
            let channels: u16 = if stereo { 2 } else { 1 };
            let samples: Vec<i16> = frames
                .iter()
                .flat_map(|&s| std::iter::repeat_n(s, usize::from(channels)))
                .collect();
            let pcm = read_wav(&wav(channels, SAMPLE_RATE, 16, &samples)).unwrap();
            prop_assert_eq!(pcm.stereo, stereo);
            prop_assert_eq!(pcm.frames as usize, frames.len());
            let expected: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
            prop_assert_eq!(pcm.bytes, expected);
        }

        #[test]
        fn any_bytes_read_as_a_sound_or_an_error_without_panicking(bytes in prop::collection::vec(any::<u8>(), 0..256)) {
            let _read = read_wav(&bytes);
        }
    }
}
