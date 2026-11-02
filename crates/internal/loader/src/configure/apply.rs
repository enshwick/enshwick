//! Writes one setting of a mod into the running world's settings file:
//! checked against its schema and by the mod's own check, written
//! atomically with the file's comments kept, and the mod told. It applies
//! when the world loads again, or at once for a mod that follows changes.
//! With no world running nothing is written: settings belong to each world.
//! The console's `enshwick config`, the Mods screen and a server's
//! administrators all change settings through here.

use crate::host::Loader;
use crate::locks::Relock;
use crate::texts;

use super::{Change, ChangeSettingError, refused};

/// Sets mod `index`'s setting `key`, a key of its settings files seen as
/// one, to `value`, TOML or for a text setting the words as typed; `who`
/// names the player for the log.
///
/// # Errors
///
/// [`ApplySettingError`]: no world, the mod, setting or value refused, or
/// the file not written.
pub(crate) fn apply(
    loader: &Loader,
    index: usize,
    key: &str,
    value: &str,
    who: &str,
) -> Result<Change, ApplySettingError> {
    let record = loader.mods.get(index).ok_or(ApplySettingError::NoMod)?;
    let id = record.id.as_str();
    let files = record.settings_files.relock().clone();
    if files.is_empty() {
        return Err(ApplySettingError::NoSchema { id: id.to_owned() });
    }
    let world = crate::worlds::current().ok_or(ApplySettingError::NoWorld)?;
    let (file, inner) = crate::worlds::split_key(key, &files);
    let Some(settings) = files.iter().find(|f| f.file == file) else {
        return Err(ApplySettingError::NoSchema { id: id.to_owned() });
    };
    let path = crate::worlds::world_file(id, file, &world);
    let text = enshwick::files::read_to_string_if_present(&path)
        .map_err(|source| ApplySettingError::Write {
            path: path.clone(),
            source,
        })?
        .unwrap_or_else(|| settings.template.clone());
    let shown = crate::worlds::shown_file(id, &files, key);
    let change = super::set(&shown, &settings.schema, &text, inner, value)?;
    crate::host::check_settings(index, file, &change.text)?;
    crate::watch::write_noted(&path, || crate::worlds::write_target(&path, &change.text)).map_err(
        |source| ApplySettingError::Write {
            path: path.clone(),
            source,
        },
    )?;
    crate::log!(INFO, "{who} set {id} {key} = {}", change.value);
    // The change is not the asking player's: the mod's handler reads no
    // reader's language.
    texts::for_nobody(|| crate::worlds::changed(loader, index));
    Ok(change)
}

/// Why a setting of a mod was not written.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ApplySettingError {
    /// The loader has no mod of the index.
    #[error("no such mod")]
    NoMod,
    /// The mod has no settings.
    #[error("{id} has no settings")]
    NoSchema { id: String },
    /// No world runs, and settings belong to each world.
    #[error("no world runs")]
    NoWorld,
    /// The file the change goes to was not read or written.
    #[error("{} was not written: {source}", path.display())]
    Write {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// The schema refuses the change.
    #[error(transparent)]
    Change(#[from] ChangeSettingError),
    /// The mod refuses the settings with the change.
    #[error(transparent)]
    Check(#[from] crate::host::CheckSettingsError),
}

impl ApplySettingError {
    /// The reply for the player, in the game's language.
    pub(crate) fn reply(&self) -> String {
        match self {
            Self::NoMod => texts::fill("refused.no-mod", &[]),
            Self::NoSchema { id } => texts::fill("remote.no-schema", &[("id", id)]),
            Self::NoWorld => texts::fill("mods.settings.in-world", &[]),
            Self::Write { path, source } => texts::fill(
                "refused.write",
                &[
                    ("path", &path.display().to_string()),
                    ("error", &source.to_string()),
                ],
            ),
            Self::Change(error) => error.reply(),
            Self::Check(error) => refused(&error.reply()),
        }
    }
}
