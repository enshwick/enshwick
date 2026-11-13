//! Writes one setting of a mod into the running world's settings file:
//! checked against its schema and by the mod's own check, written
//! atomically with the file's comments kept, and the mod told. It applies
//! when the world loads again, or at once for a mod that follows changes.
//! With no world running nothing is written: settings belong to each world.
//! The console's `enshwick config`, the Mods screen, a server's
//! administrators and the mod itself all change settings through here.

use std::sync::Mutex;
use std::sync::atomic::Ordering;
use std::thread::ThreadId;

use enshwick_core::abi::{Status, Str};
use enshwick_core::config::MAX_NESTED_WRITES;

use crate::engine::loader::Loader;
use crate::engine::mod_index::ModIndex;
use crate::engine::texts;
use enshwick_base::Relock;

use super::{Change, ChangeSettingError, Changer, refused};

/// Sets mod `index`'s setting `key`, a key of its settings files seen as
/// one, to `value`, TOML or for a text setting the words as typed; `who`
/// names the player for the log. A person never sets a secret.
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
    write(loader, index, None, key, value, Writer::Person(who))
}

/// Sets `key` of mod `index`'s own settings file `file` to `value`, one
/// TOML value, for the mod itself: a secret too, which the log never shows.
///
/// # Errors
///
/// [`ApplySettingError`] as for [`apply`].
pub(crate) fn apply_own(
    loader: &Loader,
    index: usize,
    file: &str,
    key: &str,
    value: &str,
) -> Result<Change, ApplySettingError> {
    write(loader, index, Some(file), key, value, Writer::Mod)
}

/// Who writes a setting.
#[derive(Clone, Copy, Debug)]
enum Writer<'a> {
    /// A person, named for the log.
    Person(&'a str),
    /// The mod whose setting it is.
    Mod,
}

/// The settings writes in flight.
struct Writes {
    /// Held from reading a file to writing it back, so two writers never
    /// lose each other's change.
    file: Mutex<()>,
    /// Each thread inside a write, once per write it nests, and whether that
    /// write holds `file`.
    inside: Mutex<Vec<(ThreadId, bool)>>,
}

static WRITES: Writes = Writes {
    file: Mutex::new(()),
    inside: Mutex::new(Vec::new()),
};

impl Writes {
    /// Enters a write on thread `me`: refused inside one of its writes that
    /// holds the file, as from the mod's own check, and past
    /// [`MAX_NESTED_WRITES`].
    fn enter(&self, me: ThreadId) -> Result<Inside<'_>, ApplySettingError> {
        let mut inside = self.inside.relock();
        let mine = || inside.iter().filter(|(thread, _)| *thread == me);
        if mine().any(|&(_, holds)| holds) || mine().count() >= MAX_NESTED_WRITES {
            return Err(ApplySettingError::Nested);
        }
        inside.push((me, false));
        Ok(Inside { writes: self, me })
    }
}

/// A thread inside a write, which leaves it when dropped.
struct Inside<'a> {
    writes: &'a Writes,
    me: ThreadId,
}

impl Inside<'_> {
    /// Notes whether this write holds the file.
    fn holding(&self, holds: bool) {
        let mut inside = self.writes.inside.relock();
        if let Some(entry) = inside
            .iter_mut()
            .rev()
            .find(|(thread, _)| *thread == self.me)
        {
            entry.1 = holds;
        }
    }
}

impl Drop for Inside<'_> {
    fn drop(&mut self) {
        let mut inside = self.writes.inside.relock();
        if let Some(at) = inside.iter().rposition(|(thread, _)| *thread == self.me) {
            inside.remove(at);
        }
    }
}

/// Writes `key` of `file`, or of the files seen as one when `file` is
/// `None`, for `by`.
fn write(
    loader: &Loader,
    index: usize,
    file: Option<&str>,
    key: &str,
    value: &str,
    by: Writer<'_>,
) -> Result<Change, ApplySettingError> {
    let record = loader.mods.get(index).ok_or(ApplySettingError::NoMod)?;
    let id = record.id.as_str();
    let files = record.settings_files.relock().clone();
    if files.is_empty() {
        return Err(ApplySettingError::NoSchema { id: id.to_owned() });
    }
    let world = crate::base::worlds::current().ok_or(ApplySettingError::NoWorld)?;
    let (file, inner) = file.map_or_else(
        || crate::base::worlds::split_key(key, &files),
        |file| (file, key),
    );
    let Some(settings) = files.iter().find(|f| f.file == file) else {
        return Err(ApplySettingError::NoSchema { id: id.to_owned() });
    };
    let path = crate::base::worlds::world_file(id, file, &world);
    let shown = format!("{id}/{file}.toml");
    let changer = match by {
        Writer::Person(_) => Changer::Person,
        Writer::Mod => Changer::Mod,
    };
    let inside = WRITES.enter(std::thread::current().id())?;
    let change = {
        let _file = WRITES.file.relock();
        inside.holding(true);
        let text = enshwick_core::files::read_to_string_if_present(&path)
            .map_err(|source| ApplySettingError::Write {
                path: path.clone(),
                source,
            })?
            .unwrap_or_else(|| settings.template.clone());
        let change = super::set(&shown, &settings.schema, &text, inner, value, changer)?;
        check_settings(index, file, &change.text)?;
        crate::base::watch::write_noted(&path, || {
            crate::base::worlds::write_target(&path, &change.text)
        })
        .map_err(|source| ApplySettingError::Write {
            path: path.clone(),
            source,
        })?;
        inside.holding(false);
        change
    };
    let written = if change.secret {
        texts::fill("config.secret", &[])
    } else {
        change.value.clone()
    };
    match by {
        Writer::Person(who) => crate::log!(INFO, "{who} set {id} {key} = {written}"),
        Writer::Mod => crate::log!(INFO, "{id} set its own {shown} {inner} = {written}"),
    }
    // The change is not the asking player's: the mod's handler reads no
    // reader's language.
    texts::for_nobody(|| crate::base::worlds::changed(loader, index));
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
    Check(#[from] CheckSettingsError),
    /// A write from inside the mod's own check, or nested past
    /// [`MAX_NESTED_WRITES`] through change handlers.
    #[error(
        "a settings write inside the mod's check or past {MAX_NESTED_WRITES} writes nested in one another"
    )]
    Nested,
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
            Self::Nested => texts::fill("refused.by-mod", &[]),
        }
    }

    /// The answer to a mod's write of its own setting, as
    /// `HostApi::config_set` states it.
    pub(crate) fn status(&self) -> Status {
        match self {
            Self::NoWorld => Status::UNAVAILABLE,
            Self::NoSchema { .. } | Self::Change(ChangeSettingError::Find(_)) => Status::MISSING,
            Self::NoMod
            | Self::Change(
                ChangeSettingError::BadValue { .. }
                | ChangeSettingError::Refused { .. }
                | ChangeSettingError::NoElement { .. },
            ) => Status::BAD_ARGS,
            Self::Change(
                ChangeSettingError::Secret { .. }
                | ChangeSettingError::Toml { .. }
                | ChangeSettingError::Unwritable { .. },
            )
            | Self::Write { .. }
            | Self::Check(_)
            | Self::Nested => Status::REFUSED,
        }
    }

    /// Why, for the log, never quoting the value, which may be secret.
    pub(crate) fn logged(&self) -> String {
        match self {
            Self::Change(ChangeSettingError::BadValue { field, .. }) => {
                format!("{} does not take the value", field.key)
            }
            // The parser's own text quotes the file's line, which may hold a secret.
            Self::Change(ChangeSettingError::Toml { id, source }) => {
                format!("the settings file of {id} is no TOML: {}", source.message())
            }
            other => other.to_string(),
        }
    }
}

/// Runs mod `index`'s check of `text`, a new text of its settings file
/// `file`; `Ok` for a file without one.
pub(crate) fn check_settings(
    index: usize,
    file: &str,
    text: &str,
) -> Result<(), CheckSettingsError> {
    let Some(record) = crate::engine::loader::loader()
        .and_then(|loader| loader.mods.get(index))
        .filter(|record| !record.off.load(Ordering::Acquire))
    else {
        return Ok(());
    };
    let found = record
        .checks
        .relock()
        .iter()
        .find(|(known, _, _)| known == file)
        .map(|&(_, callback, user)| (callback, user));
    let Some((callback, user)) = found else {
        return Ok(());
    };
    let mut why = Str::EMPTY;
    // SAFETY: the mod's check with its own pointer; the text lives for the
    // call, and the safe layer catches its panics.
    let status = crate::engine::profile::call(
        ModIndex::new(index),
        crate::engine::profile::Feature::Events,
        || unsafe {
            callback(
                Str::new(text),
                &raw mut why,
                core::ptr::with_exposed_provenance_mut(user),
            )
        },
    );
    if status.is_ok() {
        return Ok(());
    }
    // SAFETY: the reason stays valid until the mod's next check.
    let why = unsafe { why.as_str() };
    Err(why.map_or(CheckSettingsError::Unexplained, |why| {
        CheckSettingsError::Refused {
            reason: texts::mod_reason(record.texts.relock().as_ref(), why),
        }
    }))
}

/// Why a mod refuses a new settings text.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub(crate) enum CheckSettingsError {
    /// The mod gave no reason.
    #[error("the mod refuses the settings")]
    Unexplained,
    /// The mod's reason, in the reader's language.
    #[error("the mod refuses the settings: {reason}")]
    Refused { reason: String },
}

impl CheckSettingsError {
    /// The refusal as the reader reads it.
    pub(crate) fn reply(&self) -> String {
        match self {
            Self::Unexplained => texts::fill("refused.by-mod", &[]),
            Self::Refused { reason } => reason.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn writes() -> Writes {
        Writes {
            file: Mutex::new(()),
            inside: Mutex::new(Vec::new()),
        }
    }

    #[test]
    fn a_write_from_inside_the_mods_check_is_refused_not_locked_out() {
        let writes = writes();
        let me = std::thread::current().id();
        let outer = writes.enter(me).unwrap();
        outer.holding(true);
        assert!(matches!(writes.enter(me), Err(ApplySettingError::Nested)));
        outer.holding(false);
        let handler = writes.enter(me).unwrap();
        drop((handler, outer));
        assert!(writes.inside.relock().is_empty());
    }

    #[test]
    fn writes_nest_through_change_handlers_only_a_few_deep() {
        let writes = writes();
        let me = std::thread::current().id();
        let nested: Vec<_> = (0..MAX_NESTED_WRITES)
            .map(|_| writes.enter(me).unwrap())
            .collect();
        assert!(matches!(writes.enter(me), Err(ApplySettingError::Nested)));
        let other = std::thread::scope(|scope| {
            scope
                .spawn(|| writes.enter(std::thread::current().id()).is_ok())
                .join()
                .unwrap()
        });
        assert!(other, "another thread's write is not nested");
        drop(nested);
        assert!(writes.enter(me).is_ok());
    }

    #[test]
    fn a_settings_file_that_does_not_read_is_logged_without_its_lines() {
        let source = "token = \"hunter2\nlimit = 4\n"
            .parse::<toml_edit::DocumentMut>()
            .unwrap_err();
        assert!(source.to_string().contains("hunter2"), "{source}");
        let error = ApplySettingError::Change(ChangeSettingError::Toml {
            id: "you.trading-post".to_owned(),
            source,
        });
        let logged = error.logged();
        assert!(!logged.contains("hunter2"), "{logged}");
        assert!(
            logged.starts_with("the settings file of you.trading-post is no TOML: "),
            "{logged}"
        );
    }
}
