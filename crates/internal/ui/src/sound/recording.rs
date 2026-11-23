//! A mod's recordings: a file in its folder, read and checked against the
//! mod's bounds at init, encoded as `FramelessQOA` on background threads, and
//! served as one of the game's sounds, new or in place of a vanilla one.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use super::SoundId;
use super::decode::{self, Samples, Shape};
use super::qoa;
use crate::Off;
use crate::abi::{Level, ResourceKey, Side};
use crate::content::types;
use crate::locks::Relock as _;
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

/// The game's `FramelessQOA` sounds a mod's recordings build from: one held
/// inline and one streamed, each with its streamed data, found once.
#[derive(Clone, Debug)]
pub(super) struct Bases {
    inline: (ResourceKey, Value),
    streamed: (ResourceKey, Value),
}

/// What a mod's sounds hold while it initialises: the encoded bytes of its
/// recordings so far, the game's sounds they build from, and the threads
/// that encode them; it drops when the mod's init ends.
#[derive(Default)]
struct Budget {
    bytes: u64,
    bases: Option<Bases>,
    encoder: Option<Arc<Encoder>>,
}

impl Drop for Budget {
    fn drop(&mut self) {
        if let Some(encoder) = &self.encoder {
            let mut queue = encoder.queue.relock();
            queue.ended = true;
            encoder.log_when_done(&mut queue);
        }
    }
}

/// A mod's background encoding: up to one thread per core, each taking the
/// mod's waiting recordings in turn.
struct Encoder {
    queue: Mutex<Queue>,
    host: crate::Host,
}

/// The recordings waiting for an encoding thread, and the count and time
/// of those done, logged once.
#[derive(Default)]
struct Queue {
    waiting: VecDeque<Arc<Job>>,
    threads: usize,
    started: usize,
    done: usize,
    took: Duration,
    /// Whether the mod's init has ended, so no recording joins.
    ended: bool,
    logged: bool,
}

impl Queue {
    /// The one line that sums the mod's encoding, once its init has ended
    /// and none is left; `None` before and after.
    fn summary(&mut self) -> Option<String> {
        if !self.ended || self.logged || self.done < self.started {
            return None;
        }
        self.logged = true;
        Some(format!(
            "encoded {} recordings in {} ms of work",
            self.done,
            self.took.as_millis()
        ))
    }
}

impl Encoder {
    fn new(host: crate::Host) -> Self {
        Self {
            queue: Mutex::default(),
            host,
        }
    }

    /// Queues `job`, and starts a thread for it while fewer than the
    /// machine's cores run; one no thread takes is encoded by its first read.
    fn start(self: &Arc<Self>, job: &Arc<Job>) {
        let mut queue = self.queue.relock();
        queue.waiting.push_back(Arc::clone(job));
        queue.started += 1;
        let cores = std::thread::available_parallelism().map_or(1, usize::from);
        if queue.threads >= cores {
            return;
        }
        let encoder = Arc::clone(self);
        let spawned = std::thread::Builder::new()
            .name("enshwick sound encoding".to_owned())
            .spawn(move || encoder.work());
        if spawned.is_ok() {
            queue.threads += 1;
        } else if queue.threads == 0 {
            queue.waiting.pop_back();
            queue.started -= 1;
        }
    }

    /// Encodes the waiting recordings until none is left; a panic stops
    /// only its recording, whose reads then say so.
    fn work(&self) {
        while let Some(job) = self.next() {
            let started = Instant::now();
            let _encoded = enshwick_base::host::caught(|| drop(job.encoded()));
            let mut queue = self.queue.relock();
            queue.done += 1;
            queue.took += started.elapsed();
            self.log_when_done(&mut queue);
        }
    }

    /// The next waiting recording; `None` ends the thread.
    fn next(&self) -> Option<Arc<Job>> {
        let mut queue = self.queue.relock();
        let job = queue.waiting.pop_front();
        if job.is_none() {
            queue.threads -= 1;
        }
        job
    }

    fn log_when_done(&self, queue: &mut Queue) {
        if let Some(line) = queue.summary() {
            self.host.log(Level::INFO, &line);
        }
    }
}

/// One recording's encoding: its samples until encoded, then its bytes,
/// and once they stream, only the hash the loader holds them under.
struct Job {
    state: Mutex<JobState>,
    channels: usize,
}

enum JobState {
    Samples(Vec<i16>),
    Encoded(Arc<[u8]>),
    Served([u32; 4]),
    /// The encoding panicked.
    Stopped,
}

impl Job {
    fn new(samples: Samples) -> Self {
        Self {
            channels: samples.channels,
            state: Mutex::new(JobState::Samples(samples.samples)),
        }
    }

    /// Its state with the samples encoded, here when no thread has yet; a
    /// call while a thread encodes waits for it.
    fn encoded(&self) -> MutexGuard<'_, JobState> {
        let mut state = self.state.relock();
        if matches!(*state, JobState::Samples(_)) {
            // Stopped until encoded, so a panic leaves it stopped.
            let taken = core::mem::replace(&mut *state, JobState::Stopped);
            if let JobState::Samples(samples) = taken {
                *state = JobState::Encoded(qoa::encode(&samples, self.channels).into());
            }
        }
        state
    }

    /// The encoded bytes of an inline recording.
    fn bytes(&self) -> Result<Arc<[u8]>, Off> {
        match &*self.encoded() {
            JobState::Encoded(bytes) => Ok(Arc::clone(bytes)),
            _ => Err(Off::new("its encoding stopped")),
        }
    }

    /// The hash a streamed recording's bytes are served under: served
    /// through `serve` by the first call, which then drops them, since the
    /// loader keeps its own copy.
    fn served(&self, serve: impl FnOnce(&[u8]) -> Result<[u32; 4], Off>) -> Result<[u32; 4], Off> {
        let mut state = self.encoded();
        let hash = match &*state {
            JobState::Served(hash) => return Ok(*hash),
            JobState::Encoded(bytes) => serve(bytes)?,
            JobState::Samples(_) | JobState::Stopped => {
                return Err(Off::new("its encoding stopped"));
            }
        };
        *state = JobState::Served(hash);
        Ok(hash)
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
    let held = load
        .slot::<Budget>()
        .as_ref()
        .map_or(0, |budget| budget.bytes);
    let bounds = decode::Bounds::left(held.saturating_add(pending));
    let samples = decode::recording(&bytes, shape, bounds).map_err(|e| fail(&e))?;
    let encoded =
        u64::try_from(qoa::encoded_len(samples.frames(), samples.channels)).unwrap_or(u64::MAX);
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

    /// Its length in frames at [`SAMPLE_RATE`].
    pub(super) fn frames(&self) -> usize {
        self.samples.frames()
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

/// The game's `FramelessQOA` sounds a mod's recordings build from, found
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
/// (`replace` false) built from a vanilla `FramelessQOA` sound, or a vanilla
/// one replaced in place after the patches of earlier mods, under the debug
/// name `name`. Its encoding starts now on a background thread; the game's
/// first read of the sound waits for it. A server, which has no sound type,
/// serves nothing; either side counts the bytes.
///
/// # Errors
///
/// [`Off`] on a client whose game holds no `FramelessQOA` sounds to build
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
        let (inline_data, streamed) = if self.inline {
            (
                self.job
                    .bytes()?
                    .iter()
                    .map(|&byte| Value::Uint(u64::from(byte)))
                    .collect(),
                self.streamed.clone(),
            )
        } else {
            let hash = self.job.served(serve)?;
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
        let first = job.bytes().unwrap();
        assert_eq!(first.len(), 16);
        assert!(Arc::ptr_eq(&job.bytes().unwrap(), &first), "encoded once");
    }

    #[test]
    fn a_streamed_job_serves_its_bytes_once_and_drops_them() {
        let job = Job::new(Samples {
            samples: vec![1; 40],
            channels: 1,
        });
        assert_eq!(job.served(|_| Ok([7, 0, 0, 0])).unwrap()[0], 7);
        let again = job.served(|_| unreachable!("served once")).unwrap();
        assert_eq!(again[0], 7);
        assert!(job.bytes().is_err(), "the loader holds the only copy");
    }

    #[test]
    fn a_job_whose_encoding_panicked_says_so_and_serves_nothing() {
        let job = Job {
            state: Mutex::new(JobState::Stopped),
            channels: 1,
        };
        assert!(job.bytes().is_err());
        assert!(job.served(|_| unreachable!("stopped")).is_err());
    }

    #[test]
    fn the_encoding_is_summed_once_when_init_has_ended_and_none_is_left() {
        let mut queue = Queue {
            started: 2,
            done: 2,
            ..Queue::default()
        };
        assert_eq!(queue.summary(), None, "init still adds recordings");
        queue.ended = true;
        queue.started = 3;
        assert_eq!(queue.summary(), None, "one is left");
        queue.done = 3;
        assert!(
            queue
                .summary()
                .is_some_and(|line| line.starts_with("encoded 3 recordings"))
        );
        assert_eq!(queue.summary(), None, "once");
    }
}
