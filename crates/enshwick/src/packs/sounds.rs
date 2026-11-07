//! A pack's `sounds.toml`: its own sounds from WAV files in its folder,
//! read and checked.

use serde::Deserialize;
use toml::Spanned;

use super::SOUNDS_FILE;
use super::reader::Reader;

/// `sounds.toml`.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SoundsFile {
    #[serde(default)]
    pub(super) sound: Vec<Spanned<SoundEntry>>,
}

/// One sound.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(super) struct SoundEntry {
    /// Its name in the pack; its container's content id is
    /// `<pack>:<id>`, which other mods play.
    pub(super) id: String,
    /// The WAV file, relative to the pack's folder: 16-bit PCM, mono or
    /// stereo, at 48000 Hz, at most 16 MiB and 600 s.
    pub(super) file: String,
    /// What it plays as: `sfx` when left out, `music`, `ui` or `voice`.
    pub(super) scene: Option<SceneEntry>,
    /// Its volume, from 0 to 4; 1 when left out.
    pub(super) volume: Option<f32>,
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
    #[cfg(feature = "unstable")]
    pub(super) const fn scene(self) -> crate::sound::SoundContainerScene {
        use crate::sound::SoundContainerScene;
        match self {
            Self::Sfx => SoundContainerScene::Sfx,
            Self::Music => SoundContainerScene::Music,
            Self::Ui => SoundContainerScene::Ui,
            Self::Voice => SoundContainerScene::Voice,
        }
    }
}

/// A pack's sound, checked: its full content id, file, scene and volume.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct PackSound {
    pub(super) id: String,
    pub(super) file: String,
    pub(super) scene: Option<SceneEntry>,
    pub(super) volume: Option<f32>,
}

/// The checked sounds of the pack's `sounds.toml`.
pub(super) fn sounds(reader: &mut Reader<'_>) -> Vec<PackSound> {
    let (file, text): (SoundsFile, String) = reader.file(SOUNDS_FILE);
    let text = text.as_str();
    let mut out = Vec::new();
    for entry in &file.sound {
        let at = entry.span().start;
        let e = entry.get_ref();
        reader.define(SOUNDS_FILE, text, at, &e.id, "sound");
        let inside = std::path::Path::new(&e.file)
            .components()
            .all(|part| matches!(part, std::path::Component::Normal(_)));
        if e.file.is_empty() || !inside {
            reader.error(
                SOUNDS_FILE,
                text,
                at,
                format!(
                    "sound {}: {:?} is no path inside the pack's folder",
                    e.id, e.file
                ),
            );
        }
        if let Some(volume) = e.volume
            && !(0.0..=crate::sound::MAX_VOLUME).contains(&volume)
        {
            reader.error(
                SOUNDS_FILE,
                text,
                at,
                format!("sound {}: volume {volume} is outside 0 to 4", e.id),
            );
        }
        out.push(PackSound {
            id: reader.own(&e.id),
            file: e.file.clone(),
            scene: e.scene,
            volume: e.volume,
        });
    }
    out
}
