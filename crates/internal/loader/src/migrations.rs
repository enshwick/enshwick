//! Mod migrations: what a new version of a mod changes in what older versions
//! left behind.
//!
//! The settings steps run on a mod's settings file when it registers its
//! migrations; every step runs on a world's files when the world loads, before
//! the mod reads the world's settings or opens its stores. `mod-
//! data/enshwick/migrations.toml` records what ran, for the settings file and
//! per world, so nothing runs twice; a world or settings file the mod has never
//! seen starts current. Before changing anything the loader logs what it will
//! do; in a dry run it only logs.

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

/// Why a migration step failed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum MigrationError {
    /// The settings file is no TOML.
    #[error("settings: {0}")]
    Settings(toml_edit::TomlError),
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
        reason: toml_edit::TomlError,
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
        reason: toml_edit::TomlError,
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
    Unreadable(toml_edit::TomlError),
    /// A file could not be written or removed.
    #[error("{file}: {reason}")]
    File {
        /// The file's name.
        file: String,
        /// Why.
        reason: std::io::Error,
    },
}

/// The text of another mod's settings file beside the one migrated, by that
/// mod's id; `None` when it has none.
pub type SourceFn<'a> = &'a dyn Fn(&str) -> Option<String>;

/// What ran, for the settings files and per world: mod id to migration ids;
/// and the former ids whose files each renamed mod took over.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct Record {
    /// Migrations that ran on each mod's settings file.
    pub settings: BTreeMap<String, Vec<String>>,
    /// Migrations that ran on each world, by world key.
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
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let body = toml::to_string(self).map_err(std::io::Error::other)?;
        let text = format!(
            "# Mod migrations that ran: on each mod's settings file, and per world;\n\
             # and the former ids whose files each renamed mod took over.\n\
             # The loader keeps this file; nothing listed here runs again.\n\n{body}"
        );
        let staged = path.with_extension("toml.new");
        std::fs::write(&staged, text)?;
        std::fs::rename(&staged, path)
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
    let mut doc: DocumentMut = text.parse().map_err(MigrationError::Settings)?;
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
                    new.parse().map_err(|reason| MigrationError::SettingValue {
                        key: step.from.clone(),
                        reason,
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
        .map_err(MigrationError::Unreadable)?;
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
    let Some(text) = source(id) else {
        return Ok(());
    };
    let other: DocumentMut = text
        .parse()
        .map_err(|reason| MigrationError::OtherSettings {
            id: id.to_owned(),
            reason,
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
    staged.commit()
}

/// Files a migration changes, held until every step of it succeeded, so a
/// step that fails leaves the files as they were and the migration runs
/// whole on the next load: `None` for a file it removes.
#[derive(Debug, Default)]
struct Staged {
    files: Vec<(PathBuf, Option<String>)>,
}

impl Staged {
    /// The text of `path` as the steps so far leave it.
    fn read(&self, path: &Path) -> Option<String> {
        match self.files.iter().rev().find(|(p, _)| p == path) {
            Some((_, text)) => text.clone(),
            None => std::fs::read_to_string(path).ok(),
        }
    }

    /// Whether `path` exists as the steps so far leave it.
    fn exists(&self, path: &Path) -> bool {
        match self.files.iter().rev().find(|(p, _)| p == path) {
            Some((_, text)) => text.is_some(),
            None => path.exists(),
        }
    }

    fn put(&mut self, path: PathBuf, text: Option<String>) {
        self.files.retain(|(p, _)| *p != path);
        self.files.push((path, text));
    }

    /// Writes every staged file, then removes the ones staged away.
    fn commit(self) -> Result<(), MigrationError> {
        let failed = |path: &Path, reason| MigrationError::File {
            file: path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            reason,
        };
        for (path, text) in &self.files {
            if let Some(text) = text {
                write(path, text).map_err(|e| failed(path, e))?;
            }
        }
        for (path, text) in &self.files {
            if text.is_none() && path.exists() {
                std::fs::remove_file(path).map_err(|e| failed(path, e))?;
            }
        }
        Ok(())
    }
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
                if staged.exists(&from) && !staged.exists(&to) {
                    let text = staged
                        .read(&from)
                        .ok_or_else(|| MigrationError::StoreUnreadable(step.from.clone()))?;
                    staged.put(to, Some(text));
                    staged.put(from, None);
                }
            }
            MigrationStep::MAP_STORE => {
                let path = file(&step.from);
                let Some(text) = staged.read(&path) else {
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

/// Writes `text` to `path` through a temporary file and a rename, its
/// folder made when missing.
fn write(path: &Path, text: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let staged = path.with_extension("toml.new");
    std::fs::write(&staged, text)?;
    std::fs::rename(&staged, path)
}

/// Runs the pending migrations of mod `id` on its settings file, or on a
/// world folder with its `settings.toml`, and records them in `record`;
/// returns the plan line, when anything was pending.
///
/// A settings file that is not there yet waits, unrecorded, for the loader
/// to write it. A file or world the mod has never seen starts current: only
/// the steps that take settings from other mods run, and every migration is
/// recorded.
///
/// # Errors
///
/// The plan line and why the first failing migration failed; the ones
/// before it are recorded, and all of them on a file the mod never saw.
pub fn run(
    record: &mut Record,
    target: Target<'_>,
    id: &str,
    all: &[Migration],
    map: MapFn<'_>,
    dry: bool,
) -> Result<Option<String>, (String, String)> {
    let (world, fresh) = match target {
        Target::Settings { path, .. } if !path.exists() => return Ok(None),
        Target::Settings { born, .. } => (None, born),
        Target::World { key, dir } => (Some(key), !dir.exists()),
    };
    let source = |other: &str| std::fs::read_to_string(target.source(other)).ok();
    let ran = record.ran(world, id);
    let taking: Vec<(usize, Migration)>;
    let pending: Vec<(usize, &Migration)> = if fresh {
        taking = all
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
    } else {
        pending(all, ran)
    };
    if fresh && !dry {
        ran.clear();
        ran.extend(all.iter().map(|m| m.id.clone()));
    }
    if pending.is_empty() {
        return Ok(None);
    }
    let line = plan(id, &pending, world.is_none());
    if dry {
        return Ok(Some(line));
    }
    let mut changed = false;
    for (index, migration) in pending {
        let mut staged = Staged::default();
        let applied = match target {
            Target::Settings { path, .. } => {
                stage_settings(&mut staged, path, index, migration, map, &source)
            }
            Target::World { dir, .. } => stage_settings(
                &mut staged,
                &dir.join("settings.toml"),
                index,
                migration,
                map,
                &source,
            )
            .and_then(|()| stage_stores(&mut staged, dir, index, migration, map)),
        }
        .and_then(|()| {
            changed |= !staged.files.is_empty();
            staged.commit()
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
    let text = match staged.read(path) {
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
