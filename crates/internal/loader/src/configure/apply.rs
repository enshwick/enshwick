//! Writes one setting of a mod: checked against its schema and by the mod's
//! own check, written atomically with the file's comments kept, and the
//! mod told at once. Each player's own settings go to the settings file,
//! the others to the running world's settings, or with no world running to
//! the changes waiting for the next world. The console's `enshwick config` and the Mods screen both change settings
//! through here.

use enshwick::abi::Event;
use enshwick::config::Stage;

use crate::host::Loader;
use crate::locks::Relock;
use crate::texts;
use crate::worlds::Destination;

use super::{Change, ChangeSettingError, refused};

/// Sets mod `index`'s setting `key` to `value`, TOML or for a text setting
/// the words as typed; `who` names the player for the log.
///
/// # Errors
///
/// [`ApplySettingError`]: the mod, setting or value refused, or the file
/// not written.
pub(crate) fn apply(
    loader: &Loader,
    index: usize,
    key: &str,
    value: &str,
    who: &str,
) -> Result<Change, ApplySettingError> {
    let record = loader.mods.get(index).ok_or(ApplySettingError::NoMod)?;
    let id = record.id.as_str();
    let schema = record
        .schema
        .relock()
        .clone()
        .ok_or_else(|| ApplySettingError::NoSchema { id: id.to_owned() })?;
    let field = schema.field(super::element(key).0);
    if loader.side == enshwick::abi::Side::SERVER && field.is_some_and(|f| f.stage == Stage::Client)
    {
        return Err(ApplySettingError::Client {
            id: id.to_owned(),
            key: key.to_owned(),
        });
    }
    let stage = field.map_or(Stage::Runtime, |f| f.stage);
    let destination = crate::worlds::destination(stage, crate::worlds::current().as_deref());
    let global_path = loader.configs_dir.join(format!("{id}.toml"));
    let global =
        std::fs::read_to_string(&global_path).map_err(|source| ApplySettingError::Missing {
            id: id.to_owned(),
            source,
        })?;
    let (path, text) = crate::worlds::target_text(loader, record, &destination)
        .map_err(|(path, source)| ApplySettingError::Write { path, source })?;
    let mut change = super::set(id, &schema, &text, key, value)?;
    // The mod checks what it would read with the change.
    let read = match &destination {
        Destination::Global => crate::worlds::effective(loader, record, &change.text),
        Destination::World(_) => crate::worlds::compose(&change.text, &global, &schema),
        Destination::Next => crate::worlds::overlay(&global, &change.text, None),
    };
    crate::host::check_settings(index, &read)?;
    crate::watch::write_noted(&path, || crate::worlds::write_target(&path, &change.text)).map_err(
        |source| ApplySettingError::Write {
            path: path.clone(),
            source,
        },
    )?;
    crate::log!(INFO, "{who} set {id} {key} = {}", change.value);
    change.destination = destination.clone();
    if destination != Destination::Next {
        // The change is the world's, not the asking player's: the mod's
        // handler reads no reader's language.
        texts::for_nobody(|| {
            loader.emit(Event::SETTINGS_CHANGED, Some(index), core::ptr::null());
        });
    }
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
    /// A player's own setting, which a server does not change.
    #[error("{id} {key} is each player's own")]
    Client { id: String, key: String },
    /// The mod's settings file is not there.
    #[error("mod-configs/{id}.toml does not read: {source}")]
    Missing {
        id: String,
        #[source]
        source: std::io::Error,
    },
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
            Self::NoSchema { id } => texts::fill("config.no-schema", &[("id", id)]),
            Self::Client { id, key } => texts::fill("refused.client", &[("id", id), ("key", key)]),
            Self::Missing { id, .. } => texts::fill("config.missing", &[("id", id)]),
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
