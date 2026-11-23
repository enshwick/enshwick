//! A mod's recordings: a file in its folder, read and checked against the
//! mod's bounds, and served as one of the game's sounds, new or in place of
//! a vanilla one.

use super::SoundId;
use crate::Off;
use crate::abi::{Level, ResourceKey, Side};
use crate::content::types;
use crate::resources::Value;

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
const DATA: &str = "data";
const STREAMED: &str = "streamedData";
const CONTENT_HASH: &str = "hash";
const CONTENT_HASH_PARTS: [&str; 4] = ["size", "hash0", "hash1", "hash2"];
const DEBUG_NAME: &str = "debugName";
/// How the debug name of a recording a mod replaced starts, so a later
/// replacement names the earlier one.
const REPLACED: &str = "replaced by ";

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
#[derive(Clone, Debug, Default)]
struct Budget {
    bytes: u64,
    base: Option<(ResourceKey, Value)>,
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

/// A mod's file read as a recording, its bytes not yet counted.
pub(super) struct Recording {
    pcm: Pcm,
    size: u64,
}

/// Reads the mod's `file` as a recording within the mod's bounds.
///
/// # Errors
///
/// [`Off`] naming `file` for a path outside the mod's folder, a file past
/// [`MAX_FILE_BYTES`] or past the mod's [`MAX_MOD_BYTES`], and one that is
/// no sound a mod adds.
pub(super) fn read(load: &mut crate::BaseLoad, file: &str) -> Result<Recording, Off> {
    let fail = |why: &dyn core::fmt::Display| Off::new(format!("{file}: {why}"));
    let path = file_in(&load.mod_dir()?, file).map_err(|why| Off::new(why.to_string()))?;
    let size = std::fs::metadata(&path).map_err(|e| fail(&e))?.len();
    if size > MAX_FILE_BYTES {
        return Err(fail(&format!("{size} bytes, past {MAX_FILE_BYTES}")));
    }
    let held = load
        .slot::<Budget>()
        .as_ref()
        .map_or(0, |budget| budget.bytes);
    if held + size > MAX_MOD_BYTES {
        return Err(fail(&format!(
            "the mod's sounds would pass {MAX_MOD_BYTES} bytes"
        )));
    }
    let wav = std::fs::read(&path).map_err(|e| fail(&e))?;
    let pcm = read_wav(&wav).map_err(|e| fail(&e))?;
    Ok(Recording { pcm, size })
}

/// Counts a recording's bytes against the mod's bound.
pub(super) fn count(load: &mut crate::BaseLoad, recording: &Recording) {
    load.slot::<Budget>().get_or_insert_default().bytes += recording.size;
}

/// The game's streamed uncompressed sound a mod's recordings build from, and
/// its streamed data, found once per mod.
pub(super) fn base(load: &mut crate::BaseLoad) -> Result<(ResourceKey, Value), Off> {
    if let Some(base) = load.slot::<Budget>().as_ref().and_then(|b| b.base.clone()) {
        return Ok(base);
    }
    let host = load.host();
    let sound_type = crate::resources::Registry::game()?
        .hashed(types::SOUND_RESOURCE.hash()?)
        .ok_or_else(|| Off::new("this game has no sound type"))?;
    // One of the game's few uncompressed sounds, all streamed, carries the
    // content category a served sound needs.
    let found = host
        .resource_keys(Some(sound_type))?
        .into_iter()
        .find_map(|key| {
            let value = host.game_value(key).ok()?;
            is_streamed_uncompressed(&value)
                .then(|| value.field(STREAMED).cloned())
                .flatten()
                .map(|streamed| (key, streamed))
        })
        .ok_or_else(|| Off::new("the game holds no streamed uncompressed sound to build from"))?;
    load.slot::<Budget>().get_or_insert_default().base = Some(found.clone());
    Ok(found)
}

/// Serves `recording` on a client as the game's sound `sound`: a new one
/// (`replace` false) built from a vanilla uncompressed sound, or a vanilla
/// one replaced in place after the patches of earlier mods, under the debug
/// name `name`. A server, which has no sound type, serves nothing; either
/// side counts the bytes.
///
/// # Errors
///
/// [`Off`] on a client whose game holds no streamed uncompressed sound to
/// build from, and when the loader refuses the content or the resource.
pub(super) fn serve(
    load: &mut crate::BaseLoad,
    sound: SoundId,
    recording: Recording,
    replace: bool,
    name: String,
) -> Result<(), Off> {
    if load.host().build().side != Side::CLIENT {
        count(load, &recording);
        return Ok(());
    }
    let key = types::SOUND_RESOURCE.key(sound.guid())?;
    let (base_key, streamed) = base(load)?;
    let host = load.host();
    let hash = host.serve_content(&recording.pcm.bytes)?;
    // Served bytes stay held whatever fails after them.
    count(load, &recording);
    let Recording {
        pcm: Pcm { stereo, frames, .. },
        ..
    } = recording;
    let made = Made {
        stereo,
        frames,
        hash,
        streamed,
        name,
    };
    crate::resources::serve_value(
        host,
        key,
        if replace { key } else { base_key },
        move |held| {
            if let Some(Value::Text(earlier)) = held.field(DEBUG_NAME)
                && let Some(earlier) = earlier.strip_prefix(REPLACED)
            {
                host.log(
                    Level::WARN,
                    &format!(
                        "sound {}: {} replaces the recording {earlier} gave it",
                        sound.guid(),
                        made.name
                    ),
                );
            }
            made.over(held)
        },
    )
}

/// The debug name of a vanilla recording the mod's `file` replaces.
pub(super) fn replaced_name(load: &crate::BaseLoad, file: &str) -> String {
    let folder = load.mod_dir().ok();
    let mod_name = folder
        .as_deref()
        .and_then(std::path::Path::file_name)
        .map_or_else(|| "a mod".into(), |name| name.to_string_lossy());
    format!("{REPLACED}{mod_name}/{file}")
}

/// What a served recording writes over the sound it is built on.
struct Made {
    stereo: bool,
    frames: u32,
    hash: [u32; 4],
    streamed: Value,
    name: String,
}

impl Made {
    /// `base`, a sound of the game's layout, made to hold these frames
    /// streamed as their content; [`Off`] when it lacks a field the frames
    /// need, so a renamed field serves nothing rather than the base's
    /// layout over the mod's frames.
    fn over(&self, mut base: Value) -> Result<Value, Off> {
        let nanos = i64::try_from(u64::from(self.frames) * 1_000_000_000 / u64::from(SAMPLE_RATE))
            .unwrap_or(i64::MAX);
        let mut streamed = self.streamed.clone();
        *streamed.field_mut(CONTENT_HASH).ok_or_else(|| {
            Off::new(format!("the game's sound has no {STREAMED}.{CONTENT_HASH}"))
        })? = Value::Struct(
            CONTENT_HASH_PARTS
                .iter()
                .zip(self.hash)
                .map(|(part, word)| ((*part).to_owned(), Value::Uint(u64::from(word))))
                .collect(),
        );
        let mut set = |field: &str, value: Value| {
            let slot = base
                .field_mut(field)
                .ok_or_else(|| Off::new(format!("the game's sound has no {field}")))?;
            *slot = value;
            Ok::<_, Off>(())
        };
        set(FORMAT, Value::Name(UNCOMPRESSED.to_owned()))?;
        set(
            CHANNELS,
            Value::Name(if self.stereo { STEREO } else { MONO }.to_owned()),
        )?;
        set(FRAME_COUNT, Value::Uint(u64::from(self.frames)))?;
        set(
            DURATION,
            Value::Struct(vec![(TIME_VALUE.to_owned(), Value::Int(nanos))]),
        )?;
        set(STREAMED, streamed)?;
        // A replaced vanilla sound may hold its samples inline.
        let _cleared = set(DATA, Value::List(Vec::new()));
        let _named = set(DEBUG_NAME, Value::Text(self.name.clone()));
        Ok(base)
    }
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

/// `Load`'s methods of this module on the base handle, which core's
/// forwarders call.
#[doc(hidden)]
pub trait LoadMethods {
    /// As `Load::replace_sound`.
    #[doc(hidden)]
    fn replace_sound(&mut self, sound: SoundId, file: &str) -> Result<(), Off>;
}

impl LoadMethods for crate::BaseLoad {
    fn replace_sound(&mut self, sound: SoundId, file: &str) -> Result<(), Off> {
        let guid = sound.guid();
        let fail = |why: Off| Off::new(format!("sound {guid}: {why}"));
        if !self.joins() {
            return Err(fail(Off::new("a mod with sounds runs on both sides")));
        }
        let recording = read(self, file).map_err(fail)?;
        if self.host().build().side == Side::CLIENT
            && self
                .host()
                .game_resource(types::SOUND_RESOURCE.key(guid)?)
                .is_none()
        {
            return Err(fail(Off::new("the game holds no such recording")));
        }
        let name = replaced_name(self, file);
        serve(self, sound, recording, true, name).map_err(fail)
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

    fn streamed(size: u64) -> Value {
        Value::Struct(vec![
            (
                CONTENT_HASH.into(),
                Value::Struct(vec![(CONTENT_HASH_PARTS[0].into(), Value::Uint(size))]),
            ),
            ("category".into(), Value::Uint(9)),
        ])
    }

    fn sound(format: &str, streamed_size: u64) -> Value {
        Value::Struct(vec![
            (FORMAT.into(), Value::Name(format.into())),
            (CHANNELS.into(), Value::Name(MONO.into())),
            (FRAME_COUNT.into(), Value::Uint(1)),
            (
                DURATION.into(),
                Value::Struct(vec![(TIME_VALUE.into(), Value::Int(1))]),
            ),
            (DATA.into(), Value::List(vec![Value::Uint(7)])),
            (STREAMED.into(), streamed(streamed_size)),
            (DEBUG_NAME.into(), Value::Text("waterfall".into())),
        ])
    }

    fn made() -> Made {
        Made {
            stereo: true,
            frames: SAMPLE_RATE * 2,
            hash: [192_000, 1, 2, 3],
            streamed: streamed(5),
            name: "you.horns:horn".into(),
        }
    }

    #[test]
    fn a_new_sound_takes_its_frames_channels_and_content_over_the_base() {
        let base = sound(UNCOMPRESSED, 2);
        assert!(is_streamed_uncompressed(&base));
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
            made().over(Value::Struct(renamed)).is_err(),
            "a base without a field the frames need serves nothing"
        );
        let built = made().over(base).unwrap();
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
    fn a_replaced_inline_sound_streams_the_mod_s_frames_and_holds_no_samples_of_its_own() {
        let built = made().over(sound("FramelessQOA", 0)).unwrap();
        assert!(is_streamed_uncompressed(&built));
        assert_eq!(built.field(DATA), Some(&Value::List(Vec::new())));
        assert_eq!(
            built.field(DEBUG_NAME),
            Some(&Value::Text("you.horns:horn".into()))
        );
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
