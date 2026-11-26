//! A pack's `sounds.toml`: its own sounds from WAV files in its folder,
//! changes to any sound container, and recordings it replaces, read and
//! checked.

use std::time::Duration;

use serde::Deserialize;
use toml::Spanned;

use super::SOUNDS_FILE;
use super::reader::Reader;
use crate::content::Guid;
use crate::sound::{
    GAIN_DB, MAX_NEW_TAKES, MAX_SLEEP, RandomPosition, SoundChange, SoundContainerScene,
    SoundContainerStealingMode, SoundId, Take, Takes,
};

/// `sounds.toml`.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SoundsFile {
    #[serde(default)]
    pub(super) sound: Vec<Spanned<SoundEntry>>,
    #[serde(default)]
    pub(super) change: Vec<Spanned<ChangeEntry>>,
    #[serde(default)]
    pub(super) replace: Vec<Spanned<ReplaceEntry>>,
    #[serde(default)]
    pub(super) voiceover: Vec<Spanned<VoiceoverEntry>>,
    #[serde(default)]
    pub(super) music: Vec<Spanned<MusicEntry>>,
}

/// A voiced line of the pack, or new recordings for one of the game's.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(super) struct VoiceoverEntry {
    /// Its name in the pack: the line `<pack>:<id>`, each language's
    /// container `<pack>:<id>.<language>`.
    pub(super) id: String,
    /// The game's voiced line it gives these languages, by its GUID; left
    /// out for a line of the pack's own.
    pub(super) voiceover: Option<String>,
    /// One audio file per voice language, each
    /// `{ language = "english", file = "..." }`; the languages are the
    /// game's, in snake case.
    pub(super) lines: Vec<LineEntry>,
    /// The language whose file also speaks in every language left empty.
    pub(super) fallback: Option<String>,
    /// Which channels the recordings keep, as a sound's.
    pub(super) channels: Option<ChannelsEntry>,
    /// A gain in decibels on every file, from -60 to 24.
    pub(super) gain_db: Option<f32>,
    /// Whether the line plays at the speaker's place; `true` when left out,
    /// `false` suits a lore reading.
    pub(super) has_position: Option<bool>,
}

/// One voice language's file of a voiced line.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(super) struct LineEntry {
    /// The language, such as `english` or `simplified_chinese`.
    pub(super) language: String,
    /// The file, relative to the pack's folder, as a sound's.
    pub(super) file: String,
}

/// Music of the pack, which the client's ambience plays where and when the
/// entry says.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(super) struct MusicEntry {
    /// Its name in the pack; its recordings are `<pack>:<id>.sound` on.
    pub(super) id: String,
    /// Its audio files, 1 to 32, as a sound's; one plays at a time.
    pub(super) files: Vec<String>,
    /// Which channels the recordings keep, as a sound's.
    pub(super) channels: Option<ChannelsEntry>,
    /// A gain in decibels on every file, from -60 to 24.
    pub(super) gain_db: Option<f32>,
    /// Where it plays: one of the game's environments, such as `Grassland`;
    /// everywhere when left out.
    pub(super) environment: Option<String>,
    /// Only in the fog (`true`) or only out of it; both when left out.
    pub(super) in_fog: Option<bool>,
    /// Only indoors or only outdoors; both when left out.
    pub(super) indoors: Option<bool>,
    /// Only in battle or only out of it; both when left out.
    pub(super) in_battle: Option<bool>,
    /// The hour of the day it starts, from 0 to 24; 0 when left out.
    pub(super) start_hour: Option<f32>,
    /// The hour it stops, from 0 to 24, before the start for a span over
    /// midnight; 24 when left out.
    pub(super) end_hour: Option<f32>,
    /// Its volume, from 0 to 1; 0.2, as most of the game's music, when left
    /// out.
    pub(super) volume: Option<f32>,
    /// The pause after each file, in seconds, at most 600.
    pub(super) sleep: Option<f32>,
    /// The random spread of that pause, in seconds, at most 600.
    pub(super) sleep_randomness: Option<f32>,
}

/// Declares a table of `sounds.toml` that takes its own fields and every
/// value of a sound container, flat, so a misspelt name is still named.
macro_rules! container_entry {
    ($(#[doc = $doc:literal])* $name:ident { $($(#[doc = $field_doc:literal])* $field:ident: $ty:ty,)* }) => {
        $(#[doc = $doc])*
        #[derive(Clone, Debug, Default, PartialEq, Deserialize)]
        #[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
        #[serde(deny_unknown_fields)]
        pub(super) struct $name {
            $($(#[doc = $field_doc])* pub(super) $field: $ty,)*
            /// Its volume, from 0 to 4.
            pub(super) volume: Option<f32>,
            /// The random spread of its volume, from 0 to 4.
            pub(super) volume_randomness: Option<f32>,
            /// Its pitch, from 0.25 to 4.
            pub(super) pitch: Option<f32>,
            /// The random spread of its pitch, from 0 to 4.
            pub(super) pitch_randomness: Option<f32>,
            /// The least distance a placed sound is heard at in full, from 0 to 500 metres.
            pub(super) min_distance: Option<f32>,
            /// The greatest distance a placed sound is heard at, from 0 to 500 metres.
            pub(super) max_distance: Option<f32>,
            /// Whether it plays again from the start when it ends.
            #[serde(rename = "loop")]
            pub(super) looped: Option<bool>,
            /// Its priority among sounds playing at once.
            pub(super) priority: Option<u16>,
            /// What it plays as: `sfx`, `music`, `ui` or `voice`.
            pub(super) scene: Option<SceneEntry>,
            /// Whether it plays at a place in the world.
            pub(super) has_position: Option<bool>,
            /// The pause between two plays of a looped sound, in seconds, at most 600.
            pub(super) sleep: Option<f32>,
            /// The random spread of that pause, in seconds, at most 600.
            pub(super) sleep_randomness: Option<f32>,
            /// How many of the takes played last are not picked again, at most 64.
            pub(super) avoid_repeating_last: Option<u32>,
            /// Whether a looped sound plays the same take each time round.
            pub(super) loop_same_take: Option<bool>,
            /// The most plays at once, at most 8192; 0 for no limit.
            pub(super) max_count: Option<u32>,
            /// Which play a new one stops at the limit: `oldest`, `furthest` or `ignore` (the new one does not start).
            pub(super) stealing_mode: Option<StealingModeEntry>,
            /// How far each play moves at random, in metres, at most 500 each way.
            pub(super) random_position: Option<RandomPositionEntry>,
            /// The game's `muteOnMain` flag.
            pub(super) mute_on_main: Option<bool>,
            /// Its volume on a gamepad's speaker, from 0 (off) to 4.
            pub(super) gamepad_speaker: Option<f32>,
            /// The strength of a gamepad's vibration with it, from 0 (off) to 1.
            pub(super) gamepad_vibration: Option<f32>,
        }

        impl $name {
            /// The container values this entry gives, without takes.
            fn values(&self) -> SoundChange {
                let seconds = |s: Option<f32>| s.map(|s| Duration::try_from_secs_f32(s).unwrap_or(Duration::MAX));
                let mut change = SoundChange::default();
                change.volume = self.volume;
                change.volume_randomness = self.volume_randomness;
                change.pitch = self.pitch;
                change.pitch_randomness = self.pitch_randomness;
                change.min_distance = self.min_distance;
                change.max_distance = self.max_distance;
                change.looped = self.looped;
                change.priority = self.priority;
                change.scene = self.scene.map(SceneEntry::scene);
                change.has_position = self.has_position;
                change.sleep = seconds(self.sleep);
                change.sleep_randomness = seconds(self.sleep_randomness);
                change.avoid_repeating_last = self.avoid_repeating_last;
                change.loop_same_take = self.loop_same_take;
                change.max_count = self.max_count;
                change.stealing_mode = self.stealing_mode.map(StealingModeEntry::mode);
                change.random_position = self.random_position.map(|r| RandomPosition {
                        left_right: r.left_right.unwrap_or(0.0),
                        front_back: r.front_back.unwrap_or(0.0),
                });
                change.mute_on_main = self.mute_on_main;
                change.gamepad_speaker = self.gamepad_speaker;
                change.gamepad_vibration = self.gamepad_vibration;
                change
            }
        }
    };
}

container_entry! {
    /// One sound of the pack and the container that picks among its takes.
    SoundEntry {
        /// Its name in the pack; its container's content id is
        /// `<pack>:<id>`, which other mods play, and its takes' recordings
        /// `<pack>:<id>.sound`, `<pack>:<id>.sound2` and on.
        id: String,
        /// Its one audio file, relative to the pack's folder: WAV, FLAC,
        /// Ogg Vorbis or MP3, at most 16 MiB; or `takes` for several.
        file: Option<String>,
        /// Its audio files, 1 to 32, each `{ file = "...", chance = <0 to 1> }`;
        /// the container picks one at random by their chances.
        takes: Option<Vec<NewTakeEntry>>,
        /// Which channels the recordings keep: `keep` when left out, `mono`
        /// (which suits a sound placed in the world) or `stereo`.
        channels: Option<ChannelsEntry>,
        /// A gain in decibels on every take, from -60 to 24.
        gain_db: Option<f32>,
        /// The part of each file it plays: `{ from = <seconds>, to = <seconds> }`,
        /// each left out for the file's own.
        trim: Option<TrimEntry>,
    }
}

container_entry! {
    /// A change to a sound container: the game's, another mod's or the
    /// pack's own.
    ChangeEntry {
        /// The container: a GUID, `<alias>:<name>` of a required mod, or the
        /// id of one of the pack's sounds.
        sound: String,
        /// Which recordings it plays: `{ add = [...] }` after those it holds,
        /// or `{ replace = [...] }` instead of them.
        takes: Option<TakesEntry>,
    }
}

/// A recording of the game the pack replaces wherever it plays.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(super) struct ReplaceEntry {
    /// The game's recording, by its GUID.
    pub(super) sound: String,
    /// The audio file, relative to the pack's folder, as a sound's.
    pub(super) file: String,
}

/// One audio file of a pack's sound.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(super) struct NewTakeEntry {
    /// The file, relative to the pack's folder.
    pub(super) file: String,
    /// Its chance among the sound's takes, from 0 to 1; 1 when left out.
    pub(super) chance: Option<f32>,
}

/// Which channels a pack's sound keeps.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "lowercase")]
pub(super) enum ChannelsEntry {
    /// The file's own.
    Keep,
    /// One.
    Mono,
    /// Two.
    Stereo,
}

/// The part of each file a pack's sound plays, in seconds.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(super) struct TrimEntry {
    /// Where it starts.
    pub(super) from: Option<f32>,
    /// Where it ends.
    pub(super) to: Option<f32>,
}

/// A sound container's scene.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "lowercase")]
pub(super) enum SceneEntry {
    /// A sound effect.
    Sfx,
    /// Music.
    Music,
    /// An interface sound.
    Ui,
    /// A voice.
    Voice,
}

impl SceneEntry {
    const fn scene(self) -> SoundContainerScene {
        match self {
            Self::Sfx => SoundContainerScene::Sfx,
            Self::Music => SoundContainerScene::Music,
            Self::Ui => SoundContainerScene::Ui,
            Self::Voice => SoundContainerScene::Voice,
        }
    }
}

/// Which play of a container a new one stops at its limit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "lowercase")]
pub(super) enum StealingModeEntry {
    /// The play that started first.
    Oldest,
    /// The play farthest from the listener.
    Furthest,
    /// None: the new play does not start.
    Ignore,
}

impl StealingModeEntry {
    const fn mode(self) -> SoundContainerStealingMode {
        match self {
            Self::Oldest => SoundContainerStealingMode::Oldest,
            Self::Furthest => SoundContainerStealingMode::Furthest,
            Self::Ignore => SoundContainerStealingMode::Ignore,
        }
    }
}

/// How far each play moves at random, in metres; a side left out is 0.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(super) struct RandomPositionEntry {
    /// To either side.
    pub(super) left_right: Option<f32>,
    /// Forwards and back.
    pub(super) front_back: Option<f32>,
}

/// Which recordings a container plays.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "lowercase", deny_unknown_fields)]
pub(super) enum TakesEntry {
    /// These after those it holds.
    Add(Vec<TakeEntry>),
    /// These instead of those it holds.
    Replace(Vec<TakeEntry>),
}

/// One recording a container may play.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(super) struct TakeEntry {
    /// The recording: a GUID, `<alias>:<name>.sound` of a required mod's
    /// sound, or the id of one of the pack's sounds.
    pub(super) sound: String,
    /// Its weight among the container's takes, from 0 to 1; 1 when left out.
    pub(super) chance: Option<f32>,
}

/// A pack's sound, checked: its full content id, its takes' files and
/// chances, how they become recordings, and its container values.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct PackSound {
    pub(super) id: String,
    pub(super) takes: Vec<(String, Option<f32>)>,
    pub(super) channels: Option<ChannelsEntry>,
    pub(super) gain_db: f32,
    pub(super) trim: (Duration, Option<Duration>),
    pub(super) container: SoundChange,
}

#[cfg(feature = "unstable")]
impl PackSound {
    /// The sound to add with [`Load::add_sound`](../struct.Load.html#method.add_sound) under `id`.
    pub(super) fn new_sound(&self, id: crate::content::Id) -> crate::sound::NewSound {
        use crate::sound::{NewSound, NewTake, Trim};
        let first = self
            .takes
            .first()
            .map_or_else(String::new, |(file, _)| file.clone());
        let mut sound = NewSound::new(id, first);
        sound.takes = self
            .takes
            .iter()
            .map(|(file, chance)| {
                let mut take = NewTake::new(file.clone());
                take.chance = *chance;
                take
            })
            .collect();
        sound.channels = channels(self.channels);
        sound.gain_db = self.gain_db;
        sound.trim = Trim {
            from: self.trim.0,
            to: self.trim.1,
        };
        sound.container = self.container.clone();
        sound
    }
}

/// A pack's change of a container, checked.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct PackSoundChange {
    pub(super) container: Guid,
    pub(super) change: SoundChange,
}

/// A recording a pack replaces, checked.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct PackReplace {
    pub(super) sound: SoundId,
    pub(super) file: String,
}

/// A pack's voiced line, checked: its full content id, the game's line it
/// changes, if any, and its entry.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct PackVoiceover {
    pub(super) id: String,
    pub(super) voiceover: Option<Guid>,
    pub(super) entry: VoiceoverEntry,
}

/// A pack's music, checked: its full content id, its entry and its times.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct PackMusic {
    pub(super) id: String,
    pub(super) entry: MusicEntry,
    pub(super) start: Option<Duration>,
    pub(super) end: Option<Duration>,
    pub(super) sleep: Duration,
    pub(super) sleep_randomness: Duration,
}

/// The channels `entry` names, [`Channels::Keep`](crate::sound::Channels)
/// when left out.
#[cfg(feature = "unstable")]
fn channels(entry: Option<ChannelsEntry>) -> crate::sound::Channels {
    use crate::sound::Channels;
    match entry {
        None | Some(ChannelsEntry::Keep) => Channels::Keep,
        Some(ChannelsEntry::Mono) => Channels::Mono,
        Some(ChannelsEntry::Stereo) => Channels::Stereo,
    }
}

#[cfg(feature = "unstable")]
impl PackVoiceover {
    /// The line to add or change under `id`.
    pub(super) fn new_voiceover(&self, id: crate::content::Id) -> crate::sound::NewVoiceover {
        use crate::sound::{NewVoiceover, VoiceLine};
        let e = &self.entry;
        let lines = e
            .lines
            .iter()
            .map(|l| VoiceLine::new(l.language.clone(), l.file.clone()));
        let mut line = NewVoiceover::new(id, lines.collect());
        line.fallback.clone_from(&e.fallback);
        line.channels = channels(e.channels);
        line.gain_db = e.gain_db.unwrap_or(0.0);
        line.container.has_position = e.has_position;
        line
    }
}

#[cfg(feature = "unstable")]
impl PackMusic {
    /// The music to add under `id`.
    pub(super) fn new_music(&self, id: crate::content::Id) -> crate::sound::NewMusic {
        let e = &self.entry;
        let mut music = crate::sound::NewMusic::new(id, e.files.clone());
        music.channels = channels(e.channels);
        music.gain_db = e.gain_db.unwrap_or(0.0);
        music.environment.clone_from(&e.environment);
        music.in_fog = e.in_fog;
        music.indoors = e.indoors;
        music.in_battle = e.in_battle;
        music.start_time = self.start;
        music.end_time = self.end;
        music.volume = e.volume;
        music.sleep = self.sleep;
        music.sleep_randomness = self.sleep_randomness;
        music
    }
}

/// The checked sounds, changes and replacements of the pack's
/// `sounds.toml`.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct PackSounds {
    pub(super) sounds: Vec<PackSound>,
    pub(super) changes: Vec<PackSoundChange>,
    pub(super) replaces: Vec<PackReplace>,
    pub(super) voiceovers: Vec<PackVoiceover>,
    pub(super) music: Vec<PackMusic>,
}

impl PackSounds {
    pub(super) fn is_empty(&self) -> bool {
        self.sounds.is_empty()
            && self.changes.is_empty()
            && self.replaces.is_empty()
            && self.voiceovers.is_empty()
            && self.music.is_empty()
    }

    /// Whether the pack adds recordings of its own, which need the
    /// `unstable` feature.
    #[cfg(not(feature = "unstable"))]
    pub(super) fn records(&self) -> bool {
        !self.sounds.is_empty()
            || !self.replaces.is_empty()
            || !self.voiceovers.is_empty()
            || !self.music.is_empty()
    }
}

/// Checks a gain in decibels, an error at `at` outside [`GAIN_DB`].
fn check_gain(reader: &mut Reader<'_>, text: &str, at: usize, what: &str, gain: Option<f32>) {
    if let Some(gain) = gain.filter(|g| !GAIN_DB.contains(g)) {
        reader.error(
            SOUNDS_FILE,
            text,
            at,
            format!("{what}: gain {gain} dB is outside {GAIN_DB:?}"),
        );
    }
}

/// The voiced line `e` checked, its problems at `at`.
fn voiceover(
    reader: &mut Reader<'_>,
    text: &str,
    at: usize,
    e: &VoiceoverEntry,
) -> Option<PackVoiceover> {
    let what = format!("voiceover {}", e.id);
    if e.lines.is_empty() {
        reader.error(
            SOUNDS_FILE,
            text,
            at,
            format!("{what}: give a file for a language in `lines`"),
        );
    }
    for line in &e.lines {
        check_file(reader, text, at, &what, &line.file);
    }
    check_gain(reader, text, at, &what, e.gain_db);
    let voiceover = match e.voiceover.as_deref().map(|g| (g, Guid::try_parse(g))) {
        Some((_, Some(guid))) => Some(guid),
        Some((text_guid, None)) => {
            reader.error(
                SOUNDS_FILE,
                text,
                at,
                format!("{what}: {text_guid:?} is no GUID of a game's voiced line"),
            );
            return None;
        }
        None => None,
    };
    Some(PackVoiceover {
        id: reader.own(&e.id),
        voiceover,
        entry: e.clone(),
    })
}

/// The music `e` checked, its problems at `at`.
fn music(reader: &mut Reader<'_>, text: &str, at: usize, e: &MusicEntry) -> Option<PackMusic> {
    let what = format!("music {}", e.id);
    let mut fine = true;
    let mut problem = |reader: &mut Reader<'_>, why: String| {
        fine = false;
        reader.error(SOUNDS_FILE, text, at, format!("{what}: {why}"));
    };
    if !(1..=MAX_NEW_TAKES).contains(&e.files.len()) {
        problem(
            reader,
            format!("{} files: music has 1 to {MAX_NEW_TAKES}", e.files.len()),
        );
    }
    for file in &e.files {
        check_file(reader, text, at, &what, file);
    }
    check_gain(reader, text, at, &what, e.gain_db);
    if let Some(volume) = e.volume.filter(|v| !(0.0..=1.0).contains(v)) {
        problem(reader, format!("volume {volume} is outside 0 to 1"));
    }
    let mut hour = |reader: &mut Reader<'_>, name: &str, hour: Option<f32>| match hour {
        Some(h) if (0.0..=24.0).contains(&h) => Some(Duration::from_secs_f32(h * 3600.0)),
        Some(h) => {
            problem(reader, format!("{name} {h} is outside 0 to 24"));
            None
        }
        None => None,
    };
    let start = hour(reader, "start_hour", e.start_hour);
    let end = hour(reader, "end_hour", e.end_hour);
    let mut seconds = |reader: &mut Reader<'_>, name: &str, s: Option<f32>| match s {
        Some(s) if (0.0..=MAX_SLEEP.as_secs_f32()).contains(&s) => Duration::from_secs_f32(s),
        Some(s) => {
            problem(
                reader,
                format!("{name} {s} is outside 0 to {}", MAX_SLEEP.as_secs()),
            );
            Duration::ZERO
        }
        None => Duration::ZERO,
    };
    let sleep = seconds(reader, "sleep", e.sleep);
    let sleep_randomness = seconds(reader, "sleep_randomness", e.sleep_randomness);
    fine.then(|| PackMusic {
        id: reader.own(&e.id),
        entry: e.clone(),
        start,
        end,
        sleep,
        sleep_randomness,
    })
}

/// The GUID `reference` names among sounds: a GUID, `<alias>:<name>`, or a
/// sound of the pack, with `suffix` for its recording; an error at `at`
/// otherwise.
fn sound_reference(
    reader: &mut Reader<'_>,
    text: &str,
    at: usize,
    reference: &str,
    suffix: &str,
) -> Option<Guid> {
    let own = reader.defined.get(&reader.own(reference)) == Some(&"sound");
    if Guid::try_parse(reference).is_none() && !reference.contains(':') && !own {
        reader.error(
            SOUNDS_FILE,
            text,
            at,
            format!(
                "no sound {reference:?} in this pack; name a required mod's as <alias>:<name>, or the game's by its GUID"
            ),
        );
        return None;
    }
    reader
        .reference(SOUNDS_FILE, text, at, reference, "sound", suffix)
        .map(|(guid, _)| guid)
}

/// `values` with the takes of `takes` resolved, and an error at `at` for a
/// value outside its bound.
fn checked(
    reader: &mut Reader<'_>,
    text: &str,
    at: usize,
    what: &str,
    mut values: SoundChange,
    takes: Option<&TakesEntry>,
) -> SoundChange {
    if let Some(entry) = takes {
        let (TakesEntry::Add(list) | TakesEntry::Replace(list)) = entry;
        let resolved: Vec<Take> = list
            .iter()
            .filter_map(|take| {
                let guid = sound_reference(reader, text, at, &take.sound, ".sound")?;
                let mut resolved = Take::new(SoundId::from_guid(guid));
                resolved.chance = take.chance.unwrap_or(1.0);
                Some(resolved)
            })
            .collect();
        values.takes = Some(match entry {
            TakesEntry::Add(_) => Takes::Add(resolved),
            TakesEntry::Replace(_) => Takes::Replace(resolved),
        });
    }
    if let Err(why) = enshwick_ui::__internal::check_sound_change(&values) {
        reader.error(SOUNDS_FILE, text, at, format!("{what}: {why}"));
    }
    values
}

/// Checks that `file` stays inside the pack's folder.
fn check_file(reader: &mut Reader<'_>, text: &str, at: usize, what: &str, file: &str) {
    if file.is_empty() || !crate::mods::stays_inside(file) {
        reader.error(
            SOUNDS_FILE,
            text,
            at,
            format!("{what}: {file:?} is no path inside the pack's folder"),
        );
    }
}

/// The sound `e` checked, its problems at `at`; `None` when its takes or
/// trim do not read.
fn new_sound(
    reader: &mut Reader<'_>,
    text: &str,
    at: usize,
    what: &str,
    e: &SoundEntry,
) -> Option<PackSound> {
    let problem = |reader: &mut Reader<'_>, why: String| {
        reader.error(SOUNDS_FILE, text, at, format!("{what}: {why}"));
    };
    let takes: Vec<(String, Option<f32>)> = match (&e.file, &e.takes) {
        (Some(file), None) => vec![(file.clone(), None)],
        (None, Some(takes)) => takes.iter().map(|t| (t.file.clone(), t.chance)).collect(),
        _ => {
            problem(reader, "give either `file` or `takes`".to_owned());
            return None;
        }
    };
    if !(1..=MAX_NEW_TAKES).contains(&takes.len()) {
        problem(
            reader,
            format!("{} takes: a sound has 1 to {MAX_NEW_TAKES}", takes.len()),
        );
    }
    for (file, chance) in &takes {
        check_file(reader, text, at, what, file);
        if let Some(chance) = chance.filter(|c| !(0.0..=1.0).contains(c)) {
            problem(reader, format!("{file}: chance {chance} is outside 0 to 1"));
        }
    }
    let gain_db = e.gain_db.unwrap_or(0.0);
    if !GAIN_DB.contains(&gain_db) {
        problem(reader, format!("gain {gain_db} dB is outside {GAIN_DB:?}"));
    }
    let trim = e.trim.unwrap_or(TrimEntry {
        from: None,
        to: None,
    });
    let seconds = |s: Option<f32>| s.map(Duration::try_from_secs_f32).transpose().ok();
    let (Some(start), Some(end)) = (seconds(trim.from), seconds(trim.to)) else {
        problem(reader, "a trim is seconds from the file's start".to_owned());
        return None;
    };
    let start = start.unwrap_or_default();
    if end.is_some_and(|end| end <= start) {
        problem(reader, "the trim ends before it starts".to_owned());
    }
    let container = checked(reader, text, at, what, e.values(), None);
    Some(PackSound {
        id: reader.own(&e.id),
        takes,
        channels: e.channels,
        gain_db,
        trim: (start, end),
        container,
    })
}

/// The checked sounds, changes and replacements of the pack's
/// `sounds.toml`.
pub(super) fn sounds(reader: &mut Reader<'_>) -> PackSounds {
    let (file, text): (SoundsFile, String) = reader.file(SOUNDS_FILE);
    let text = text.as_str();
    let mut out = PackSounds::default();
    for entry in &file.sound {
        reader.define(
            SOUNDS_FILE,
            text,
            entry.span().start,
            &entry.get_ref().id,
            "sound",
        );
    }
    for entry in &file.sound {
        let at = entry.span().start;
        let e = entry.get_ref();
        let what = format!("sound {}", e.id);
        if let Some(sound) = new_sound(reader, text, at, &what, e) {
            out.sounds.push(sound);
        }
    }
    for entry in &file.change {
        let at = entry.span().start;
        let e = entry.get_ref();
        let what = format!("change of {}", e.sound);
        let container = sound_reference(reader, text, at, &e.sound, "");
        let change = checked(reader, text, at, &what, e.values(), e.takes.as_ref());
        if let Some(container) = container {
            out.changes.push(PackSoundChange { container, change });
        }
    }
    for entry in &file.replace {
        let at = entry.span().start;
        let e = entry.get_ref();
        let what = format!("replacement of {}", e.sound);
        check_file(reader, text, at, &what, &e.file);
        match Guid::try_parse(&e.sound) {
            Some(guid) => out.replaces.push(PackReplace {
                sound: SoundId::from_guid(guid),
                file: e.file.clone(),
            }),
            None => reader.error(
                SOUNDS_FILE,
                text,
                at,
                format!("{what}: a replaced recording is the game's, named by its GUID"),
            ),
        }
    }
    for entry in &file.voiceover {
        let at = entry.span().start;
        let e = entry.get_ref();
        reader.define(SOUNDS_FILE, text, at, &e.id, "voiceover");
        out.voiceovers.extend(voiceover(reader, text, at, e));
    }
    for entry in &file.music {
        let at = entry.span().start;
        let e = entry.get_ref();
        reader.define(SOUNDS_FILE, text, at, &e.id, "music");
        out.music.extend(music(reader, text, at, e));
    }
    out
}
