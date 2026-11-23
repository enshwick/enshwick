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
    RandomPosition, SoundChange, SoundContainerScene, SoundContainerStealingMode, SoundId, Take,
    Takes,
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
            /// Which recordings it plays: `{ add = [...] }` after those it holds, or `{ replace = [...] }` instead of them.
            pub(super) takes: Option<TakesEntry>,
        }

        impl $name {
            /// The container values this entry gives, its takes left for
            /// [`resolve_takes`].
            fn values(&self) -> SoundChange {
                let seconds = |s: Option<f32>| s.map(|s| Duration::try_from_secs_f32(s).unwrap_or(Duration::MAX));
                SoundChange {
                    volume: self.volume,
                    volume_randomness: self.volume_randomness,
                    pitch: self.pitch,
                    pitch_randomness: self.pitch_randomness,
                    min_distance: self.min_distance,
                    max_distance: self.max_distance,
                    looped: self.looped,
                    priority: self.priority,
                    scene: self.scene.map(SceneEntry::scene),
                    has_position: self.has_position,
                    sleep: seconds(self.sleep),
                    sleep_randomness: seconds(self.sleep_randomness),
                    avoid_repeating_last: self.avoid_repeating_last,
                    loop_same_take: self.loop_same_take,
                    max_count: self.max_count,
                    stealing_mode: self.stealing_mode.map(StealingModeEntry::mode),
                    random_position: self.random_position.map(|r| RandomPosition {
                        left_right: r.left_right.unwrap_or(0.0),
                        front_back: r.front_back.unwrap_or(0.0),
                    }),
                    mute_on_main: self.mute_on_main,
                    gamepad_speaker: self.gamepad_speaker,
                    gamepad_vibration: self.gamepad_vibration,
                    takes: None,
                }
            }
        }
    };
}

container_entry! {
    /// One sound of the pack and the container that plays it.
    SoundEntry {
        /// Its name in the pack; its container's content id is
        /// `<pack>:<id>`, which other mods play, and its recording's
        /// `<pack>:<id>.sound`.
        id: String,
        /// The WAV file, relative to the pack's folder: 16-bit PCM, mono or
        /// stereo, at 48000 Hz, at most 16 MiB.
        file: String,
    }
}

container_entry! {
    /// A change to a sound container: the game's, another mod's or the
    /// pack's own.
    ChangeEntry {
        /// The container: a GUID, `<alias>:<name>` of a required mod, or the
        /// id of one of the pack's sounds.
        sound: String,
    }
}

/// A recording of the game the pack replaces wherever it plays.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(super) struct ReplaceEntry {
    /// The game's recording, by its GUID.
    pub(super) sound: String,
    /// The WAV file, relative to the pack's folder, as a sound's.
    pub(super) file: String,
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

/// A pack's sound, checked: its full content id, file and container values.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct PackSound {
    pub(super) id: String,
    pub(super) file: String,
    pub(super) container: SoundChange,
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

/// The checked sounds, changes and replacements of the pack's
/// `sounds.toml`.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct PackSounds {
    pub(super) sounds: Vec<PackSound>,
    pub(super) changes: Vec<PackSoundChange>,
    pub(super) replaces: Vec<PackReplace>,
}

impl PackSounds {
    pub(super) fn is_empty(&self) -> bool {
        self.sounds.is_empty() && self.changes.is_empty() && self.replaces.is_empty()
    }
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
                Some(Take {
                    chance: take.chance.unwrap_or(1.0),
                    ..Take::new(SoundId::from_guid(guid))
                })
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
        check_file(reader, text, at, &what, &e.file);
        let container = checked(reader, text, at, &what, e.values(), e.takes.as_ref());
        out.sounds.push(PackSound {
            id: reader.own(&e.id),
            file: e.file.clone(),
            container,
        });
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
    out
}
