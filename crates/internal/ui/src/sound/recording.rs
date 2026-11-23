//! A mod's recordings: a file in its folder, read and checked against the
//! mod's bounds at init, encoded as FramelessQOA on background threads, and
//! served as one of the game's sounds, new or in place of a vanilla one.

use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

use super::SoundId;
use super::decode::{self, Samples, Shape};
use super::qoa;
use crate::Off;
use crate::abi::{Level, ResourceKey, Side};
use crate::content::types;
use crate::locks::{Relock as _, RelockWait as _};
use crate::resources::Value;

/// The rate of every sound the game plays, and of every recording a mod
/// adds, in frames per second.
pub const SAMPLE_RATE: u32 = 48_000;
/// The largest file a mod adds as a recording, in bytes.
pub const MAX_FILE_BYTES: u64 = 16 << 20;
/// The most bytes one mod's recordings hold in the client, encoded: about
/// 46 minutes of mono, 23 of stereo.
pub const MAX_MOD_BYTES: u64 = 64 << 20;
/// The longest recording held inline, which plays at once on its first
/// play; a longer one streams as content.
pub const INLINE_FRAMES: usize = 48_000;

const FORMAT: &str = "format";
const QOA: &str = "FramelessQOA";
const CHANNELS: &str = "channelConfiguration";
/// The game's `SoundChannelConfiguration` for 1, 2, 4, 6 and 8 channels.
const CHANNEL_NAMES: [(usize, &str); 5] = [
    (1, "Mono"),
    (2, "Stereo"),
    (4, "Quadrophonic"),
    (6, "FiveDotOne"),
    (8, "SevenDotOne"),
];
const FRAME_COUNT: &str = "frameCount";
const FRAMES_PER_SECOND: &str = "framesPerSecond";
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

/// The game's FramelessQOA sounds a mod's recordings build from: one held
/// inline and one streamed, each with its streamed data, found once.
#[derive(Clone, Debug)]
pub(super) struct Bases {
    inline: (ResourceKey, Value),
    streamed: (ResourceKey, Value),
}

/// What a mod's sounds hold while it initialises: the encoded bytes of its
/// recordings so far, the game's sounds they build from, and the threads
/// that encode them.
#[derive(Default)]
struct Budget {
    bytes: u64,
    bases: Option<Bases>,
    encoder: Option<Arc<Encoder>>,
}

/// A mod's background encoding: at most as many recordings at once as the
/// machine has cores, and the time they took, logged when none is left.
struct Encoder {
    free: Mutex<usize>,
    freed: Condvar,
    work: Mutex<(usize, usize, Duration)>,
    host: crate::Host,
}

impl Encoder {
    fn new(host: crate::Host) -> Self {
        let cores = std::thread::available_parallelism().map_or(1, usize::from);
        Self {
            free: Mutex::new(cores),
            freed: Condvar::new(),
            work: Mutex::new((0, 0, Duration::ZERO)),
            host,
        }
    }

    /// Encodes `job` on a thread of its own once a core is free; when no
    /// thread starts, the first read encodes it.
    fn start(self: &Arc<Self>, job: &Arc<Job>) {
        self.work.relock().0 += 1;
        let (encoder, job) = (Arc::clone(self), Arc::clone(job));
        let spawned = std::thread::Builder::new()
            .name("enshwick sound encoding".to_owned())
            .spawn(move || {
                let mut free = encoder
                    .freed
                    .relock_wait_while(encoder.free.relock(), |free| *free == 0);
                *free -= 1;
                drop(free);
                let started = Instant::now();
                let _encoded = job.data();
                *encoder.free.relock() += 1;
                encoder.freed.notify_one();
                encoder.done(started.elapsed());
            });
        if spawned.is_err() {
            self.work.relock().0 -= 1;
        }
    }

    /// Counts one finished recording, and logs the mod's encoding once its
    /// last one so far is done.
    fn done(&self, took: Duration) {
        let mut work = self.work.relock();
        work.1 += 1;
        work.2 += took;
        if work.1 == work.0 {
            self.host.log(
                Level::INFO,
                &format!(
                    "encoded {} recordings in {} ms of work",
                    work.1,
                    work.2.as_millis()
                ),
            );
        }
    }
}

/// One recording's encoding: its samples until encoded, then its bytes,
/// and the hash they were served under once streamed.
struct Job {
    samples: Mutex<Option<Vec<i16>>>,
    channels: usize,
    /// The encoded bytes; `None` when the encoding stopped, its samples
    /// gone with a thread that panicked.
    data: OnceLock<Option<Vec<u8>>>,
    served: OnceLock<[u32; 4]>,
}

impl Job {
    fn new(samples: Samples) -> Self {
        Self {
            channels: samples.channels,
            samples: Mutex::new(Some(samples.samples)),
            data: OnceLock::new(),
            served: OnceLock::new(),
        }
    }

    /// The encoded bytes, encoding them here when no thread has yet; a
    /// call while a thread encodes waits for it.
    fn data(&self) -> Result<&[u8], Off> {
        self.data
            .get_or_init(|| {
                let samples = self.samples.relock().take()?;
                Some(qoa::encode(&samples, self.channels))
            })
            .as_deref()
            .ok_or_else(|| Off::new("its encoding stopped"))
    }
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
    samples: Samples,
    bytes: u64,
}

/// Reads the mod's `file` as a recording of `shape`, within the mod's
/// bounds and what the mod's recordings already hold, `pending` bytes of
/// this call's earlier takes included.
///
/// # Errors
///
/// [`Off`] naming `file` for a path outside the mod's folder, a file past
/// [`MAX_FILE_BYTES`], one that is no recording a mod adds, and a recording
/// that would pass the mod's [`MAX_MOD_BYTES`].
pub(super) fn read(
    load: &mut crate::BaseLoad,
    file: &str,
    shape: Shape,
    pending: u64,
) -> Result<Recording, Off> {
    let fail = |why: &dyn core::fmt::Display| Off::new(format!("{file}: {why}"));
    let path = file_in(&load.mod_dir()?, file).map_err(|why| Off::new(why.to_string()))?;
    let size = std::fs::metadata(&path).map_err(|e| fail(&e))?.len();
    if size > MAX_FILE_BYTES {
        return Err(fail(&format!("{size} bytes, past {MAX_FILE_BYTES}")));
    }
    let bytes = std::fs::read(&path).map_err(|e| fail(&e))?;
    let read = decode::read(&bytes, shape.trim).map_err(|e| fail(&e))?;
    drop(bytes);
    let samples = decode::shape(read, shape).map_err(|e| fail(&e))?;
    let encoded =
        u64::try_from(qoa::encoded_len(samples.frames(), samples.channels)).unwrap_or(u64::MAX);
    let held = load
        .slot::<Budget>()
        .as_ref()
        .map_or(0, |budget| budget.bytes);
    if held + pending + encoded > MAX_MOD_BYTES {
        return Err(fail(&format!(
            "the mod's recordings would pass {MAX_MOD_BYTES} bytes"
        )));
    }
    Ok(Recording {
        samples,
        bytes: encoded,
    })
}

impl Recording {
    /// Its encoded size, which counts against the mod's bound.
    pub(super) const fn bytes(&self) -> u64 {
        self.bytes
    }
}

/// The value of a streamed data reference with `hash` over `streamed`.
fn with_hash(mut streamed: Value, hash: [u32; 4]) -> Result<Value, Off> {
    *streamed
        .field_mut(CONTENT_HASH)
        .ok_or_else(|| Off::new(format!("the game's sound has no {STREAMED}.{CONTENT_HASH}")))? =
        Value::Struct(
            CONTENT_HASH_PARTS
                .iter()
                .zip(hash)
                .map(|(part, word)| ((*part).to_owned(), Value::Uint(u64::from(word))))
                .collect(),
        );
    Ok(streamed)
}

/// The size of the content `value`, a sound, streams; 0 for none.
fn streamed_size(value: &Value) -> u64 {
    match value
        .field(STREAMED)
        .and_then(|streamed| streamed.field(CONTENT_HASH))
        .and_then(|hash| hash.field(CONTENT_HASH_PARTS[0]))
    {
        Some(Value::Uint(size)) => *size,
        _ => 0,
    }
}

/// The game's FramelessQOA sounds a mod's recordings build from, found
/// once per mod.
pub(super) fn bases(load: &mut crate::BaseLoad) -> Result<Bases, Off> {
    if let Some(bases) = load.slot::<Budget>().as_ref().and_then(|b| b.bases.clone()) {
        return Ok(bases);
    }
    let host = load.host();
    let sound_type = crate::resources::Registry::game()?
        .hashed(types::SOUND_RESOURCE.hash()?)
        .ok_or_else(|| Off::new("this game has no sound type"))?;
    let (mut inline, mut streamed) = (None, None);
    for key in host.resource_keys(Some(sound_type))? {
        let Ok(value) = host.game_value(key) else {
            continue;
        };
        if value.field(FORMAT) != Some(&Value::Name(QOA.to_owned())) {
            continue;
        }
        let Some(reference) = value.field(STREAMED).cloned() else {
            continue;
        };
        let slot = if streamed_size(&value) > 0 {
            &mut streamed
        } else {
            &mut inline
        };
        slot.get_or_insert((key, reference));
        if inline.is_some() && streamed.is_some() {
            break;
        }
    }
    let (Some(inline), Some(streamed)) = (inline, streamed) else {
        return Err(Off::new(
            "the game holds no inline and streamed FramelessQOA sounds to build from",
        ));
    };
    let bases = Bases { inline, streamed };
    load.slot::<Budget>().get_or_insert_default().bases = Some(bases.clone());
    Ok(bases)
}

/// Counts a recording's bytes against the mod's bound.
fn count(load: &mut crate::BaseLoad, bytes: u64) {
    load.slot::<Budget>().get_or_insert_default().bytes += bytes;
}

/// Serves `recording` on a client as the game's sound `sound`: a new one
/// (`replace` false) built from a vanilla FramelessQOA sound, or a vanilla
/// one replaced in place after the patches of earlier mods, under the debug
/// name `name`. Its encoding starts now on a background thread; the game's
/// first read of the sound waits for it. A server, which has no sound type,
/// serves nothing; either side counts the bytes.
///
/// # Errors
///
/// [`Off`] on a client whose game holds no FramelessQOA sounds to build
/// from, and when the loader refuses the resource.
pub(super) fn serve(
    load: &mut crate::BaseLoad,
    sound: SoundId,
    recording: Recording,
    replace: bool,
    name: String,
) -> Result<(), Off> {
    count(load, recording.bytes);
    if load.host().build().side != Side::CLIENT {
        return Ok(());
    }
    let key = types::SOUND_RESOURCE.key(sound.guid())?;
    let bases = bases(load)?;
    let host = load.host();
    let frames = recording.samples.frames();
    let channels = recording.samples.channels;
    let inline = frames <= INLINE_FRAMES;
    let (base, streamed) = if inline { bases.inline } else { bases.streamed };
    let job = Arc::new(Job::new(recording.samples));
    let encoder = Arc::clone(
        load.slot::<Budget>()
            .get_or_insert_default()
            .encoder
            .get_or_insert_with(|| Arc::new(Encoder::new(host))),
    );
    encoder.start(&job);
    let made = Made {
        frames,
        channels,
        streamed,
        name,
        job,
        inline,
    };
    crate::resources::serve_value(host, key, if replace { key } else { base }, move |held| {
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
        made.over(held, |data| host.serve_content(data))
    })
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
    frames: usize,
    channels: usize,
    streamed: Value,
    name: String,
    job: Arc<Job>,
    inline: bool,
}

impl Made {
    /// `base`, a sound of the game's layout, made to hold this recording,
    /// its bytes served through `serve` when it streams; [`Off`] when it
    /// lacks a field the recording needs, so a renamed field serves nothing
    /// rather than the base's layout over the mod's samples.
    fn over(
        &self,
        base: Value,
        serve: impl FnOnce(&[u8]) -> Result<[u32; 4], Off>,
    ) -> Result<Value, Off> {
        let data = self.job.data()?;
        let (inline_data, streamed) = if self.inline {
            (
                data.iter()
                    .map(|&byte| Value::Uint(u64::from(byte)))
                    .collect(),
                self.streamed.clone(),
            )
        } else {
            // The loader answers the same bytes with the same hash, so two
            // first reads at once serve them twice at no cost.
            let hash = if let Some(hash) = self.job.served.get() {
                *hash
            } else {
                let hash = serve(data)?;
                *self.job.served.get_or_init(|| hash)
            };
            (Vec::new(), with_hash(self.streamed.clone(), hash)?)
        };
        self.write(base, Value::List(inline_data), streamed)
    }

    fn write(&self, mut base: Value, data: Value, streamed: Value) -> Result<Value, Off> {
        let frames = u64::try_from(self.frames).unwrap_or(u64::MAX);
        let nanos =
            i64::try_from(frames * 1_000_000_000 / u64::from(SAMPLE_RATE)).unwrap_or(i64::MAX);
        let channels = CHANNEL_NAMES
            .iter()
            .find(|(count, _)| *count == self.channels)
            .map(|(_, name)| *name)
            .ok_or_else(|| Off::new(format!("{} channels", self.channels)))?;
        let mut set = |field: &str, value: Value| {
            let slot = base
                .field_mut(field)
                .ok_or_else(|| Off::new(format!("the game's sound has no {field}")))?;
            *slot = value;
            Ok::<_, Off>(())
        };
        set(FORMAT, Value::Name(QOA.to_owned()))?;
        set(CHANNELS, Value::Name(channels.to_owned()))?;
        set(FRAMES_PER_SECOND, Value::Uint(u64::from(SAMPLE_RATE)))?;
        set(FRAME_COUNT, Value::Uint(frames))?;
        set(
            DURATION,
            Value::Struct(vec![(TIME_VALUE.to_owned(), Value::Int(nanos))]),
        )?;
        set(DATA, data)?;
        set(STREAMED, streamed)?;
        let _named = set(DEBUG_NAME, Value::Text(self.name.clone()));
        Ok(base)
    }
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
        let recording = read(self, file, Shape::default(), 0).map_err(fail)?;
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
    use super::*;

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

    fn sound(size: u64) -> Value {
        Value::Struct(vec![
            (FORMAT.into(), Value::Name(QOA.into())),
            (CHANNELS.into(), Value::Name("Mono".into())),
            (FRAMES_PER_SECOND.into(), Value::Uint(48_000)),
            (FRAME_COUNT.into(), Value::Uint(1)),
            (
                DURATION.into(),
                Value::Struct(vec![(TIME_VALUE.into(), Value::Int(1))]),
            ),
            (DATA.into(), Value::List(vec![Value::Uint(7)])),
            (STREAMED.into(), streamed(size)),
            (DEBUG_NAME.into(), Value::Text("waterfall".into())),
        ])
    }

    fn made(frames: usize, channels: usize, inline: bool) -> Made {
        let samples = Samples {
            samples: vec![100; frames * channels],
            channels,
        };
        Made {
            frames,
            channels,
            streamed: streamed(0),
            name: "you.horns:horn".into(),
            job: Arc::new(Job::new(samples)),
            inline,
        }
    }

    #[test]
    fn an_inline_recording_holds_its_encoded_bytes_and_streams_nothing() {
        let built = made(30, 2, true)
            .over(sound(5), |_| unreachable!("inline"))
            .unwrap();
        assert_eq!(built.field(CHANNELS), Some(&Value::Name("Stereo".into())));
        assert_eq!(built.field(FRAME_COUNT), Some(&Value::Uint(30)));
        let Some(Value::List(data)) = built.field(DATA) else {
            panic!("no data");
        };
        assert_eq!(data.len(), qoa::encoded_len(30, 2));
        assert_eq!(streamed_size(&built), 0);
        assert_eq!(
            built.field(DURATION).and_then(|d| d.field(TIME_VALUE)),
            Some(&Value::Int(625_000))
        );
        let Value::Struct(fields) = sound(5) else {
            unreachable!("built as a struct")
        };
        let renamed = fields
            .into_iter()
            .map(|(name, value)| {
                (
                    if name == FRAME_COUNT {
                        "frames".into()
                    } else {
                        name
                    },
                    value,
                )
            })
            .collect();
        assert!(
            made(30, 1, true)
                .over(Value::Struct(renamed), |_| unreachable!("inline"))
                .is_err(),
            "a base without a field the recording needs serves nothing"
        );
    }

    #[test]
    fn a_recording_of_a_channel_count_the_game_lacks_serves_nothing() {
        assert!(
            made(30, 3, true)
                .over(sound(5), |_| unreachable!("inline"))
                .is_err()
        );
    }

    #[test]
    fn a_job_encodes_once_whoever_asks_first() {
        let job = Job::new(Samples {
            samples: vec![1; 40],
            channels: 1,
        });
        let first = job.data().unwrap().to_vec();
        assert_eq!(first.len(), 16);
        assert_eq!(job.data().unwrap(), first.as_slice());
    }
}
