//! Mod migrations: what a new version of a mod changes in what older versions
//! left behind.
//!
//! The settings steps run on a mod's settings file when it registers its
//! migrations; every step runs on a world's files when the world loads, before
//! the mod reads the world's settings or opens its stores. `mod-
//! data/enshwick/migrations.toml` records what ran on the settings files, and
//! each world folder keeps its own record, [`WORLD_RECORD`], so nothing runs
//! twice, even on a world copied to another install. A settings file or world
//! the mod has never seen starts current. Before changing anything the loader
//! logs what it will do; in a dry run it only logs.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use enshwick::abi::MigrationStep;
use enshwick::content::Guid;
use toml_edit::DocumentMut;

/// The record file, under the `mod-data` folder.
pub const RECORD: &str = "enshwick/migrations.toml";

/// Whether migrations only log what they would do.
static DRY_RUN: AtomicBool = AtomicBool::new(false);

/// Sets whether migrations only log, from the loader's settings.
pub fn set_dry_run(dry: bool) {
    DRY_RUN.store(dry, Ordering::Relaxed);
}

/// Whether migrations only log what they would do.
#[must_use]
pub fn dry_run() -> bool {
    DRY_RUN.load(Ordering::Relaxed)
}

/// One step, owned by the loader.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    /// A [`MigrationStep`] kind.
    pub kind: u32,
    /// The setting or store read, or renamed from.
    pub from: String,
    /// The setting or store renamed to.
    pub to: String,
    /// The old GUID of renamed content.
    pub from_guid: [u8; 16],
    /// The new GUID of renamed content.
    pub to_guid: [u8; 16],
}

impl Step {
    /// The step in words, for the plan line.
    #[must_use]
    pub fn describe(&self) -> String {
        match self.kind {
            MigrationStep::RENAME_SETTING => {
                format!("rename setting {} to {}", self.from, self.to)
            }
            MigrationStep::MAP_SETTING => format!("change setting {}", self.from),
            MigrationStep::REMOVE_SETTING => format!("remove setting {}", self.from),
            MigrationStep::TAKE_SETTING => format!("take setting {} from {}", self.to, self.from),
            MigrationStep::RENAME_STORE => format!("rename store {} to {}", self.from, self.to),
            MigrationStep::MAP_STORE => format!("change store {}", self.from),
            MigrationStep::RENAME_CONTENT => format!(
                "read content {} as {}",
                Guid(self.from_guid),
                Guid(self.to_guid)
            ),
            other => format!("unknown step {other}"),
        }
    }

    /// Whether the step changes settings.
    #[must_use]
    pub fn on_settings(&self) -> bool {
        matches!(
            self.kind,
            MigrationStep::RENAME_SETTING
                | MigrationStep::MAP_SETTING
                | MigrationStep::REMOVE_SETTING
                | MigrationStep::TAKE_SETTING
        )
    }

    /// Whether the step takes a setting from another mod's files.
    #[must_use]
    pub fn takes(&self) -> bool {
        self.kind == MigrationStep::TAKE_SETTING
    }
}

/// One migration, owned by the loader.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Migration {
    /// The migration's id.
    pub id: String,
    /// Its steps, in order.
    pub steps: Vec<Step>,
}

/// Runs a step's map: migration index, step index and input, to the output
/// or why it failed.
pub type MapFn<'a> = &'a dyn Fn(usize, usize, &str) -> Result<String, MapError>;

/// Why a mod's map gave no value.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum MapError {
    /// The migration or step index does not fit the ABI.
    #[error("too many steps")]
    TooManySteps,
    /// The map wrote text that is not UTF-8.
    #[error("the result is not UTF-8")]
    NotUtf8,
    /// The map refused without saying why, with its status.
    #[error("refused ({0})")]
    Refused(u32),
    /// The map refused and said why.
    #[error("{0}")]
    Said(String),
}

/// Why the record of migrations that ran does not read.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum RecordError {
    /// The file is empty, as a write cut short leaves it.
    #[error("the file is empty")]
    Empty,
    /// The file is no record.
    #[error("{0}")]
    Malformed(#[from] toml::de::Error),
    /// The file does not read.
    #[error("{0}")]
    Io(#[from] std::io::Error),
}

/// Where a settings text stops being TOML, told without any value from it.
///
/// It names the line, the key that line starts with when it starts outside
/// every string, and the parser's reason, which holds no text of the file.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error(
    "line {line}{}: {reason}",
    .key.as_ref().map_or_else(String::new, |key| format!(", key {key}"))
)]
pub struct SyntaxError {
    line: usize,
    key: Option<String>,
    reason: String,
}

impl SyntaxError {
    /// The place of `error` in the settings file `text` it was parsed from.
    fn in_document(text: &str, error: &toml_edit::TomlError) -> Self {
        let mut found = Self::in_value(text, error);
        found.key = error.span().and_then(|span| key_of_line(text, span.start));
        found
    }

    /// The place of `error` in `text`, one value: no key to name.
    fn in_value(text: &str, error: &toml_edit::TomlError) -> Self {
        let start = error.span().map_or(0, |span| span.start);
        let before = text.as_bytes().get(..start).unwrap_or(text.as_bytes());
        Self {
            line: before.split(|&b| b == b'\n').count(),
            key: None,
            reason: error.message().trim_end().to_owned(),
        }
    }
}

/// The plain key the line holding byte `at` of `text` starts with; `None`
/// unless the text before that line is a whole document, which proves the
/// line does not start inside a string, an array or an inline table.
fn key_of_line(text: &str, at: usize) -> Option<String> {
    let head = text.get(..at)?;
    let line_start = head.rfind('\n').map_or(0, |i| i + 1);
    text[..line_start].parse::<DocumentMut>().ok()?;
    let (key, _) = text[line_start..].split_once('=')?;
    let key = key.trim();
    let plain = |c: char| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | ' ');
    (!key.is_empty() && key.chars().all(plain)).then(|| key.to_owned())
}

/// Why a migration step failed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum MigrationError {
    /// The settings file is no TOML.
    #[error("settings: {0}")]
    Settings(SyntaxError),
    /// The mod's map gave no value for a setting.
    #[error("setting {key}: {reason}")]
    SettingMap {
        /// The setting.
        key: String,
        /// Why the map gave none.
        reason: MapError,
    },
    /// The map's value for a setting is no TOML value.
    #[error("setting {key}: the new value: {reason}")]
    SettingValue {
        /// The setting.
        key: String,
        /// Why the value does not parse.
        reason: SyntaxError,
    },
    /// A setting taken from another mod is not `<mod id>:<key>`.
    #[error("setting {0}: not <mod id>:<key>")]
    NotModKey(String),
    /// Another mod's settings file, read for a take, is no TOML.
    #[error("settings of {id}: {reason}")]
    OtherSettings {
        /// The other mod.
        id: String,
        /// Why its file does not parse.
        reason: SyntaxError,
    },
    /// A part of the path to a taken setting is a value, not a table.
    #[error("setting {key}: {part} is not a table")]
    NotTable {
        /// The setting.
        key: String,
        /// The part that is no table.
        part: String,
    },
    /// A store to rename does not read.
    #[error("store {0}: unreadable")]
    StoreUnreadable(String),
    /// The mod's map gave no value for a store.
    #[error("store {store}: {reason}")]
    StoreMap {
        /// The store.
        store: String,
        /// Why the map gave none.
        reason: MapError,
    },
    /// The migrated settings would not read back, such as a key nested
    /// deeper than the reader takes; the file is left as it was.
    #[error("settings after the migration: {0}")]
    Unreadable(SyntaxError),
    /// Another mod's settings file, read for a take, is there but does not
    /// read.
    #[error("settings of {id}: {reason}")]
    OtherFile {
        /// The other mod.
        id: String,
        /// Why its file does not read.
        reason: std::io::Error,
    },
    /// A file could not be read, written or removed.
    #[error("{file}: {reason}")]
    File {
        /// The file's name.
        file: String,
        /// Why.
        reason: std::io::Error,
    },
    /// The world's record of the migrations that ran on it does not read,
    /// so nothing runs: it could run a migration twice.
    #[error("{WORLD_RECORD}: {0}")]
    WorldRecord(RecordError),
    /// The shared record does not read, and the world has no record of its
    /// own, so nothing runs: it could run a migration twice.
    #[error("mod-data/{RECORD}: {0}")]
    SharedRecord(String),
    /// A commit failed and its files could not all be put back; the backup
    /// keeps them, and the next load tries again.
    #[error("{first}; putting the files back failed too: {restore}; they are kept in {backup}")]
    NotRestored {
        /// Why the commit failed.
        first: Box<MigrationError>,
        /// Why putting the files back failed.
        restore: Box<MigrationError>,
        /// The backup folder.
        backup: String,
    },
}

/// The text of another mod's settings file beside the one migrated, by that
/// mod's id; `None` when it has none.
pub type SourceFn<'a> = &'a dyn Fn(&str) -> std::io::Result<Option<String>>;

/// What ran, for the settings files and per world: mod id to migration ids;
/// and the former ids whose files each renamed mod took over.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct Record {
    /// Migrations that ran on each mod's settings file.
    pub settings: BTreeMap<String, Vec<String>>,
    /// Migrations that ran on each world, by world key, from before each
    /// world kept its own record: read until the world has one.
    pub worlds: BTreeMap<String, BTreeMap<String, Vec<String>>>,
    /// The former ids each mod carried its files over from.
    pub carried: BTreeMap<String, Vec<String>>,
}

impl Record {
    /// Reads the record in `data_dir`; an empty one when there is none.
    ///
    /// # Errors
    ///
    /// When the file is there but does not read, or is empty, as a write cut
    /// short leaves it: an empty record would run every migration again.
    pub fn read(data_dir: &Path) -> Result<Self, RecordError> {
        match std::fs::read_to_string(data_dir.join(RECORD)) {
            Ok(text) => Self::parse(&text),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.into()),
        }
    }

    /// Reads the record's text.
    ///
    /// # Errors
    ///
    /// When the text is no record, or is empty, as a write cut short leaves
    /// it: an empty record would run every migration again.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader::migrations::{Record, RecordError};
    ///
    /// let record = Record::parse("[settings]\n\"you.zones\" = [\"radius\"]\n")?;
    /// assert_eq!(record.settings["you.zones"], ["radius"]);
    /// assert!(matches!(Record::parse(" \n"), Err(RecordError::Empty)));
    /// # Ok::<(), RecordError>(())
    /// ```
    pub fn parse(text: &str) -> Result<Self, RecordError> {
        if text.trim().is_empty() {
            return Err(RecordError::Empty);
        }
        Ok(toml::from_str(text)?)
    }

    /// Writes the record to `data_dir`, through a temporary file.
    ///
    /// # Errors
    ///
    /// When the file cannot be written.
    pub fn write(&self, data_dir: &Path) -> std::io::Result<()> {
        let path = data_dir.join(RECORD);
        let body = toml::to_string(self).map_err(std::io::Error::other)?;
        let text = format!(
            "# Mod migrations that ran on each mod's settings file (each world keeps its own);\n\
             # and the former ids whose files each renamed mod took over.\n\
             # The loader keeps this file; nothing listed here runs again.\n\n{body}"
        );
        enshwick::files::write_atomically(&path, text)
    }

    /// Counts every migration that ran under the former id `old` as run
    /// under `id` too, and records `old` as carried over to `id`.
    pub fn take_over(&mut self, id: &str, old: &str) {
        let lists = std::iter::once(&mut self.settings).chain(self.worlds.values_mut());
        for list in lists {
            let Some(ran) = list.get(old).cloned() else {
                continue;
            };
            let now = list.entry(id.to_owned()).or_default();
            for migration in ran {
                if !now.contains(&migration) {
                    now.push(migration);
                }
            }
        }
        let carried = self.carried.entry(id.to_owned()).or_default();
        if !carried.iter().any(|o| o == old) {
            carried.push(old.to_owned());
        }
    }

    /// The list of migrations that ran for mod `id` on the settings file,
    /// or on the world `world`.
    pub fn ran(&mut self, world: Option<&str>, id: &str) -> &mut Vec<String> {
        let list = match world {
            None => &mut self.settings,
            Some(key) => self.worlds.entry(key.to_owned()).or_default(),
        };
        list.entry(id.to_owned()).or_default()
    }

    /// What ran for mod `id` on the settings file, or on the world `world`;
    /// `None` when nothing is recorded.
    #[must_use]
    pub fn entry(&self, world: Option<&str>, id: &str) -> Option<&Vec<String>> {
        match world {
            None => self.settings.get(id),
            Some(key) => self.worlds.get(key)?.get(id),
        }
        .filter(|ran| !ran.is_empty())
    }

    /// Sets what ran for mod `id` on the settings file or the world `world`,
    /// `None` or an empty list removing the entry; whether that changed the
    /// record.
    pub fn set_entry(&mut self, world: Option<&str>, id: &str, ran: Option<Vec<String>>) -> bool {
        let ran = ran.filter(|ran| !ran.is_empty());
        if self.entry(world, id) == ran.as_ref() {
            return false;
        }
        match (ran, world) {
            (Some(ran), _) => *self.ran(world, id) = ran,
            (None, None) => drop(self.settings.remove(id)),
            (None, Some(key)) => {
                if let Some(mods) = self.worlds.get_mut(key) {
                    mods.remove(id);
                    if mods.is_empty() {
                        self.worlds.remove(key);
                    }
                }
            }
        }
        true
    }
}

/// The migrations of `all` not in `ran`, in order, with their indices.
#[must_use]
pub fn pending<'a>(all: &'a [Migration], ran: &[String]) -> Vec<(usize, &'a Migration)> {
    all.iter()
        .enumerate()
        .filter(|(_, m)| !ran.contains(&m.id))
        .collect()
}

/// The plan line: each pending migration with its steps, `only_settings`
/// for the settings file.
#[must_use]
pub fn plan(id: &str, pending: &[(usize, &Migration)], only_settings: bool) -> String {
    pending
        .iter()
        .map(|(_, m)| {
            let steps: Vec<String> = m
                .steps
                .iter()
                .filter(|s| !only_settings || s.on_settings())
                .map(Step::describe)
                .collect();
            let steps = if steps.is_empty() {
                "nothing here".to_owned()
            } else {
                steps.join(", ")
            };
            format!("{id} {}: {steps}", m.id)
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// `text` with the settings steps of migration `index` applied; `source`
/// reads the files of the mods a setting is taken from.
///
/// # Errors
///
/// Why a step failed: the file or a value is no TOML, or the mod's map
/// refused.
pub fn apply_settings(
    text: &str,
    index: usize,
    migration: &Migration,
    map: MapFn<'_>,
    source: SourceFn<'_>,
) -> Result<String, MigrationError> {
    let mut doc: DocumentMut = text
        .parse()
        .map_err(|e| MigrationError::Settings(SyntaxError::in_document(text, &e)))?;
    for (step_index, step) in migration.steps.iter().enumerate() {
        match step.kind {
            MigrationStep::TAKE_SETTING => take(&mut doc, &step.from, &step.to, source)?,
            MigrationStep::RENAME_SETTING if step.from.contains('.') || step.to.contains('.') => {
                move_key(doc.as_table_mut(), &step.from, &step.to);
            }
            MigrationStep::RENAME_SETTING => rename_key(doc.as_table_mut(), &step.from, &step.to),
            MigrationStep::REMOVE_SETTING => {
                crate::worlds::remove_at(doc.as_table_mut(), &step.from);
            }
            MigrationStep::MAP_SETTING => {
                let Some(value) = doc
                    .get_mut(&step.from)
                    .and_then(toml_edit::Item::as_value_mut)
                else {
                    continue;
                };
                let old = value.clone().decorated("", "").to_string();
                let new = map(index, step_index, old.trim()).map_err(|reason| {
                    MigrationError::SettingMap {
                        key: step.from.clone(),
                        reason,
                    }
                })?;
                let mut parsed: toml_edit::Value =
                    new.parse().map_err(|e| MigrationError::SettingValue {
                        key: step.from.clone(),
                        reason: SyntaxError::in_value(&new, &e),
                    })?;
                *parsed.decor_mut() = value.decor().clone();
                *value = parsed;
            }
            _ => {}
        }
    }
    let migrated = doc.to_string();
    migrated
        .parse::<DocumentMut>()
        .map_err(|e| MigrationError::Unreadable(SyntaxError::in_document(&migrated, &e)))?;
    Ok(migrated)
}

/// Sets `to` in `doc` from `from`, `<mod id>:<key>`, in the file `source`
/// reads for that mod, the comments on `to` kept; nothing when that mod has
/// no such value. Both keys may be dotted paths into tables, such as
/// `comfort.multiplier`; a missing table on the way to `to` is made.
fn take(
    doc: &mut DocumentMut,
    from: &str,
    to: &str,
    source: SourceFn<'_>,
) -> Result<(), MigrationError> {
    let Some((id, key)) = from.split_once(':') else {
        return Err(MigrationError::NotModKey(from.to_owned()));
    };
    let read = source(id).map_err(|reason| MigrationError::OtherFile {
        id: id.to_owned(),
        reason,
    })?;
    let Some(text) = read else {
        return Ok(());
    };
    let other: DocumentMut = text.parse().map_err(|e| MigrationError::OtherSettings {
        id: id.to_owned(),
        reason: SyntaxError::in_document(&text, &e),
    })?;
    let mut item = other.as_item();
    for part in key.split('.') {
        match item.get(part) {
            Some(next) => item = next,
            None => return Ok(()),
        }
    }
    let Some(value) = item.as_value() else {
        return Ok(());
    };
    let mut value = value.clone().decorated(" ", "");
    let (path, leaf) = to.rsplit_once('.').map_or(("", to), |(p, l)| (p, l));
    let mut table = doc.as_table_mut();
    for part in path.split('.').filter(|p| !p.is_empty()) {
        let next = table
            .entry(part)
            .or_insert_with(|| toml_edit::Item::Table(toml_edit::Table::new()));
        table = next
            .as_table_mut()
            .ok_or_else(|| MigrationError::NotTable {
                key: to.to_owned(),
                part: part.to_owned(),
            })?;
    }
    match table.get_mut(leaf).and_then(toml_edit::Item::as_value_mut) {
        Some(old) => {
            *value.decor_mut() = old.decor().clone();
            *old = value;
        }
        None => {
            table.insert(leaf, toml_edit::Item::Value(value));
        }
    }
    Ok(())
}

/// Moves the setting at the dotted key `from` to the dotted key `to`, into
/// or out of a table; nothing when `from` is missing or `to` is taken.
fn move_key(table: &mut toml_edit::Table, from: &str, to: &str) {
    if crate::worlds::value_at(table, to).is_some() {
        return;
    }
    let Some(value) = crate::worlds::value_at(table, from).cloned() else {
        return;
    };
    if crate::worlds::set_at(table, to, value.decorated(" ", "")) {
        crate::worlds::remove_at(table, from);
    }
}

/// Renames the key `from` of `table` to `to` in its place, comments kept;
/// nothing when `from` is missing or `to` is taken.
fn rename_key(table: &mut toml_edit::Table, from: &str, to: &str) {
    if !table.contains_key(from) || table.contains_key(to) {
        return;
    }
    let entries: Vec<(toml_edit::Key, toml_edit::Item)> = table
        .iter()
        .filter_map(|(name, _)| {
            let (key, item) = table.get_key_value(name)?;
            Some((key.clone(), item.clone()))
        })
        .collect();
    table.clear();
    for (key, item) in entries {
        if key.get() == from {
            let mut renamed = toml_edit::Key::new(to);
            *renamed.leaf_decor_mut() = key.leaf_decor().clone();
            table.insert_formatted(&renamed, item);
        } else {
            table.insert_formatted(&key, item);
        }
    }
}

/// Applies the store steps of migration `index` to the world folder `dir`.
///
/// # Errors
///
/// Why a step failed: a file could not be read, renamed or written, or the
/// mod's map refused.
pub fn apply_stores(
    dir: &Path,
    index: usize,
    migration: &Migration,
    map: MapFn<'_>,
) -> Result<(), MigrationError> {
    let mut staged = Staged::default();
    stage_stores(&mut staged, dir, index, migration, map)?;
    staged.commit(None)
}

/// Stages the store steps of migration `index` on the world folder `dir`.
fn stage_stores(
    staged: &mut Staged,
    dir: &Path,
    index: usize,
    migration: &Migration,
    map: MapFn<'_>,
) -> Result<(), MigrationError> {
    let file = |name: &str| dir.join(format!("{name}.toml"));
    for (step_index, step) in migration.steps.iter().enumerate() {
        match step.kind {
            MigrationStep::RENAME_STORE => {
                let (from, to) = (file(&step.from), file(&step.to));
                if staged.exists(&from)? && !staged.exists(&to)? {
                    let text = staged
                        .read(&from)?
                        .ok_or_else(|| MigrationError::StoreUnreadable(step.from.clone()))?;
                    staged.put(to, Some(text));
                    staged.put(from, None);
                }
            }
            MigrationStep::MAP_STORE => {
                let path = file(&step.from);
                let Some(text) = staged.read(&path)? else {
                    continue;
                };
                let new =
                    map(index, step_index, &text).map_err(|reason| MigrationError::StoreMap {
                        store: step.from.clone(),
                        reason,
                    })?;
                staged.put(path, Some(new));
            }
            _ => {}
        }
    }
    Ok(())
}

/// Runs the pending migrations of mod `id` on its settings file, recorded
/// in `record`, or on a world folder with its `settings.toml`, recorded in
/// the folder; returns the plan line, when anything was pending.
///
/// A settings file that is not there yet waits, unrecorded, for the loader
/// to write it. A file or world the mod has never seen starts current: only
/// the steps that take settings from other mods run, and every migration is
/// recorded. On a world each migration commits all or nothing with its
/// record, keeping a copy of the files it changed (see [`BACKUPS`]).
///
/// # Errors
///
/// The plan line and why the first failing migration failed; the ones
/// before it are recorded, and all of them on a file the mod never saw. On
/// a world whose record does not read nothing runs, and the plan is empty.
pub fn run(
    record: &mut Record,
    target: Target<'_>,
    id: &str,
    all: &[Migration],
    map: MapFn<'_>,
    dry: bool,
) -> Result<Option<String>, (String, String)> {
    match target {
        Target::Settings { path, born } => {
            if !path.exists() {
                return Ok(None);
            }
            run_on_settings(record.ran(None, id), path, born, id, all, map, dry)
        }
        Target::World { key, dir } => run_on_world(record, key, dir, id, all, map, dry),
    }
}

/// The pending migrations of `all` after `ran`, or on a file or world the
/// mod has never seen, only their steps that take settings from other mods,
/// in `taking`.
fn pending_of<'a>(
    all: &'a [Migration],
    ran: &[String],
    fresh: bool,
    taking: &'a mut Vec<(usize, Migration)>,
) -> Vec<(usize, &'a Migration)> {
    if !fresh {
        return pending(all, ran);
    }
    *taking = all
        .iter()
        .enumerate()
        .map(|(index, m)| {
            let steps = m.steps.iter().filter(|s| s.takes()).cloned().collect();
            (
                index,
                Migration {
                    id: m.id.clone(),
                    steps,
                },
            )
        })
        .filter(|(_, m)| !m.steps.is_empty())
        .collect();
    taking.iter().map(|(index, m)| (*index, m)).collect()
}

/// [`run`] on a mod's settings file, recorded in `ran`.
fn run_on_settings(
    ran: &mut Vec<String>,
    path: &Path,
    fresh: bool,
    id: &str,
    all: &[Migration],
    map: MapFn<'_>,
    dry: bool,
) -> Result<Option<String>, (String, String)> {
    let source = |other: &str| {
        enshwick::files::read_to_string_if_present(&path.with_file_name(format!("{other}.toml")))
    };
    let mut taking = Vec::new();
    let pending = pending_of(all, ran, fresh, &mut taking);
    if fresh && !dry {
        ran.clear();
        ran.extend(all.iter().map(|m| m.id.clone()));
    }
    if pending.is_empty() {
        return Ok(None);
    }
    let line = plan(id, &pending, true);
    if dry {
        return Ok(Some(line));
    }
    let mut changed = false;
    for (index, migration) in pending {
        let mut staged = Staged::default();
        let applied =
            stage_settings(&mut staged, path, index, migration, map, &source).and_then(|()| {
                changed |= !staged.files.is_empty();
                staged.commit(None)
            });
        if let Err(reason) = applied {
            return Err((line, format!("migration {}: {reason}", migration.id)));
        }
        if !fresh {
            ran.push(migration.id.clone());
        }
    }
    Ok((changed || !fresh).then_some(line))
}

/// [`run`] on the world `key` in the mod's folder for it, `dir`, recorded in
/// the world's own record there; `record`'s entry for the world, kept from
/// before worlds had their own, counts until the world's record is written,
/// and then goes.
fn run_on_world(
    record: &mut Record,
    key: &str,
    dir: &Path,
    id: &str,
    all: &[Migration],
    map: MapFn<'_>,
    dry: bool,
) -> Result<Option<String>, (String, String)> {
    let failed = |reason: MigrationError| (String::new(), reason.to_string());
    let fresh = !dir
        .try_exists()
        .map_err(|e| failed(staged::failed(dir, e)))?;
    let own = if fresh {
        None
    } else {
        WorldRecord::read(dir).map_err(|e| failed(MigrationError::WorldRecord(e)))?
    };
    let shared = record.entry(Some(key), id);
    let mut ran = match (&own, shared) {
        (Some(own), _) => own.ran.clone(),
        (None, Some(shared)) => shared.clone(),
        (None, None) => Vec::new(),
    };
    if !fresh && !dry {
        recover(dir, &ran).map_err(failed)?;
    }
    let source = |other: &str| {
        enshwick::files::read_to_string_if_present(&Target::World { key, dir }.source(other))
    };
    let mut taking = Vec::new();
    let pending = pending_of(all, &ran, fresh, &mut taking);
    let line = (!pending.is_empty()).then(|| plan(id, &pending, false));
    if dry {
        return Ok(line);
    }
    let unrecorded = own.is_none() || fresh;
    if fresh {
        ran = all.iter().map(|m| m.id.clone()).collect();
    }
    let mut changed = false;
    for (index, migration) in pending {
        let mut staged = Staged::default();
        let applied = stage_settings(
            &mut staged,
            &dir.join("settings.toml"),
            index,
            migration,
            map,
            &source,
        )
        .and_then(|()| stage_stores(&mut staged, dir, index, migration, map))
        .and_then(|()| {
            changed |= !staged.files.is_empty();
            if !fresh {
                ran.push(migration.id.clone());
            }
            staged.record(dir, WorldRecord { ran: ran.clone() });
            staged.commit(Some((&backup_of(dir, &migration.id), &migration.id)))
        });
        if let Err(reason) = applied {
            let line = line.unwrap_or_default();
            return Err((line, format!("migration {}: {reason}", migration.id)));
        }
    }
    if unrecorded && line.is_none() {
        let mut staged = Staged::default();
        staged.record(dir, WorldRecord { ran });
        staged
            .commit(None)
            .map_err(|reason| (line.clone().unwrap_or_default(), reason.to_string()))?;
    }
    record.set_entry(Some(key), id, None);
    Ok(line.filter(|_| changed || !fresh))
}

/// Stages a migration's settings steps on the file at `path`: on an empty
/// file when there is none and the migration takes settings from another
/// mod.
fn stage_settings(
    staged: &mut Staged,
    path: &Path,
    index: usize,
    migration: &Migration,
    map: MapFn<'_>,
    source: SourceFn<'_>,
) -> Result<(), MigrationError> {
    if !migration.steps.iter().any(Step::on_settings) {
        return Ok(());
    }
    let text = match staged.read(path)? {
        Some(text) => text,
        None if migration.steps.iter().any(Step::takes) => String::new(),
        None => return Ok(()),
    };
    let new = apply_settings(&text, index, migration, map, source)?;
    if new != text {
        staged.put(path.to_path_buf(), Some(new));
    }
    Ok(())
}

/// What migrations run on.
#[derive(Clone, Copy, Debug)]
pub enum Target<'a> {
    /// A mod's settings file, `mod-configs/<id>.toml`.
    Settings {
        /// The file.
        path: &'a Path,
        /// Whether the loader wrote it from the mod's defaults in this
        /// process, so the mod has never seen it.
        born: bool,
    },
    /// A world: its key and the mod's folder for it.
    World {
        /// The world key.
        key: &'a str,
        /// `mod-data/<id>/worlds/<key>`.
        dir: &'a Path,
    },
}

impl Target<'_> {
    /// The settings file the mod `other` keeps for the same target.
    #[must_use]
    pub fn source(&self, other: &str) -> PathBuf {
        match *self {
            Target::Settings { path, .. } => path.with_file_name(format!("{other}.toml")),
            Target::World { key, dir } => dir
                .ancestors()
                .nth(3)
                .unwrap_or(dir)
                .join(other)
                .join("worlds")
                .join(key)
                .join("settings.toml"),
        }
    }
}

#[cfg(windows)]
pub(crate) use live::{register, renamed, settings_written, world_loaded};

#[cfg(windows)]
mod live;

mod staged;

pub use staged::{BACKUPS, WORLD_RECORD, WorldRecord};
use staged::{Staged, backup_of, recover};

/// Each old GUID to the end of its chain of renames, A to C for A to B and
/// B to C; a cycle stops where it would come back.
#[cfg(any(windows, test))]
fn follow_renames(
    direct: &rustc_hash::FxHashMap<[u8; 16], [u8; 16]>,
) -> rustc_hash::FxHashMap<[u8; 16], [u8; 16]> {
    direct
        .iter()
        .map(|(&from, &first)| {
            let mut to = first;
            for _ in 0..direct.len() {
                match direct.get(&to) {
                    Some(&next) if next != from => to = next,
                    _ => break,
                }
            }
            (from, to)
        })
        .collect()
}

#[cfg(test)]
mod tests;
