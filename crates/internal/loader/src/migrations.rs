//! Mod migrations: what a new version of a mod changes in what older
//! versions left behind. The settings steps run on a mod's settings file
//! when it registers its migrations; every step runs on a world's files when
//! the world loads, before the mod reads the world's settings or opens its
//! stores. `mod-data/enshwick/migrations.toml` records what ran, for the
//! settings file and per world, so nothing runs twice; a world or settings
//! file the mod has never seen starts current. Before changing anything the
//! loader logs what it will do; in a dry run it only logs.

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
        )
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
pub type MapFn<'a> = &'a dyn Fn(usize, usize, &str) -> Result<String, String>;

/// What ran, for the settings files and per world: mod id to migration ids.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct Record {
    /// Migrations that ran on each mod's settings file.
    pub settings: BTreeMap<String, Vec<String>>,
    /// Migrations that ran on each world, by world key.
    pub worlds: BTreeMap<String, BTreeMap<String, Vec<String>>>,
}

impl Record {
    /// Reads the record in `data_dir`; an empty one when there is none.
    ///
    /// # Errors
    ///
    /// When the file is there but does not read, or is empty, as a write cut
    /// short leaves it: an empty record would run every migration again.
    pub fn read(data_dir: &Path) -> Result<Self, String> {
        match std::fs::read_to_string(data_dir.join(RECORD)) {
            Ok(text) if text.trim().is_empty() => Err("the file is empty".to_owned()),
            Ok(text) => toml::from_str(&text).map_err(|e| e.to_string()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.to_string()),
        }
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
            "# Mod migrations that ran: on each mod's settings file, and per world.\n\
             # The loader keeps this file; a migration listed here never runs again.\n\n{body}"
        );
        let staged = path.with_extension("toml.new");
        std::fs::write(&staged, text)?;
        std::fs::rename(&staged, path)
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

/// `text` with the settings steps of migration `index` applied.
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
) -> Result<String, String> {
    let mut doc: DocumentMut = text.parse().map_err(|e| format!("settings: {e}"))?;
    for (step_index, step) in migration.steps.iter().enumerate() {
        match step.kind {
            MigrationStep::RENAME_SETTING => rename_key(doc.as_table_mut(), &step.from, &step.to),
            MigrationStep::REMOVE_SETTING => {
                let _gone = doc.remove(&step.from);
            }
            MigrationStep::MAP_SETTING => {
                let Some(value) = doc
                    .get_mut(&step.from)
                    .and_then(toml_edit::Item::as_value_mut)
                else {
                    continue;
                };
                let old = value.clone().decorated("", "").to_string();
                let new = map(index, step_index, old.trim())
                    .map_err(|e| format!("setting {}: {e}", step.from))?;
                let mut parsed: toml_edit::Value = new
                    .parse()
                    .map_err(|e| format!("setting {}: the new value: {e}", step.from))?;
                *parsed.decor_mut() = value.decor().clone();
                *value = parsed;
            }
            _ => {}
        }
    }
    Ok(doc.to_string())
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
) -> Result<(), String> {
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
    fn commit(self) -> Result<(), String> {
        let name = |path: &Path| path.file_name().map(|n| n.to_string_lossy().into_owned());
        for (path, text) in &self.files {
            if let Some(text) = text {
                write(path, text)
                    .map_err(|e| format!("{}: {e}", name(path).unwrap_or_default()))?;
            }
        }
        for (path, text) in &self.files {
            if text.is_none() && path.exists() {
                std::fs::remove_file(path)
                    .map_err(|e| format!("{}: {e}", name(path).unwrap_or_default()))?;
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
) -> Result<(), String> {
    let file = |name: &str| dir.join(format!("{name}.toml"));
    for (step_index, step) in migration.steps.iter().enumerate() {
        match step.kind {
            MigrationStep::RENAME_STORE => {
                let (from, to) = (file(&step.from), file(&step.to));
                if staged.exists(&from) && !staged.exists(&to) {
                    let text = staged
                        .read(&from)
                        .ok_or_else(|| format!("store {}: unreadable", step.from))?;
                    staged.put(to, Some(text));
                    staged.put(from, None);
                }
            }
            MigrationStep::MAP_STORE => {
                let path = file(&step.from);
                let Some(text) = staged.read(&path) else {
                    continue;
                };
                let new = map(index, step_index, &text)
                    .map_err(|e| format!("store {}: {e}", step.from))?;
                staged.put(path, Some(new));
            }
            _ => {}
        }
    }
    Ok(())
}

/// Writes `text` to `path` through a temporary file and a rename.
fn write(path: &Path, text: &str) -> std::io::Result<()> {
    let staged = path.with_extension("toml.new");
    std::fs::write(&staged, text)?;
    std::fs::rename(&staged, path)
}

/// Runs the pending migrations of mod `id` on its settings file `path`, or
/// on the world folder `world_dir` with its `settings.toml`, and records
/// them in `record`; returns the plan line, when anything was pending.
///
/// A file or world the mod has never seen starts current: every migration
/// is recorded and none runs.
///
/// # Errors
///
/// The plan line and why the first failing migration failed; the ones
/// before it are recorded.
pub fn run(
    record: &mut Record,
    target: Target<'_>,
    id: &str,
    all: &[Migration],
    map: MapFn<'_>,
    dry: bool,
) -> Result<Option<String>, (String, String)> {
    let (world, fresh) = match target {
        Target::Settings(path) => (None, !path.exists()),
        Target::World { key, dir } => (Some(key), !dir.exists()),
    };
    let ran = record.ran(world, id);
    if fresh {
        if !dry {
            ran.clear();
            ran.extend(all.iter().map(|m| m.id.clone()));
        }
        return Ok(None);
    }
    let pending: Vec<(usize, &Migration)> = pending(all, ran);
    if pending.is_empty() {
        return Ok(None);
    }
    let line = plan(id, &pending, world.is_none());
    if dry {
        return Ok(Some(line));
    }
    for (index, migration) in pending {
        let mut staged = Staged::default();
        let applied = match target {
            Target::Settings(path) => stage_settings(&mut staged, path, index, migration, map),
            Target::World { dir, .. } => stage_settings(
                &mut staged,
                &dir.join("settings.toml"),
                index,
                migration,
                map,
            )
            .and_then(|()| stage_stores(&mut staged, dir, index, migration, map)),
        }
        .and_then(|()| staged.commit());
        if let Err(reason) = applied {
            return Err((line, format!("migration {}: {reason}", migration.id)));
        }
        ran.push(migration.id.clone());
    }
    Ok(Some(line))
}

/// Stages a migration's settings steps on the file at `path`, if any.
fn stage_settings(
    staged: &mut Staged,
    path: &Path,
    index: usize,
    migration: &Migration,
    map: MapFn<'_>,
) -> Result<(), String> {
    if !migration.steps.iter().any(Step::on_settings) {
        return Ok(());
    }
    let Some(text) = staged.read(path) else {
        return Ok(());
    };
    let new = apply_settings(&text, index, migration, map)?;
    if new != text {
        staged.put(path.to_path_buf(), Some(new));
    }
    Ok(())
}

/// What migrations run on.
#[derive(Clone, Copy, Debug)]
pub enum Target<'a> {
    /// A mod's settings file.
    Settings(&'a Path),
    /// A world: its key and the mod's folder for it.
    World {
        /// The world key.
        key: &'a str,
        /// `mod-data/<id>/worlds/<key>`.
        dir: &'a Path,
    },
}

#[cfg(windows)]
pub(crate) use live::{register, renamed, world_loaded};

#[cfg(windows)]
mod live {
    use std::sync::{Arc, Mutex, OnceLock, PoisonError};

    use enshwick::abi::{Level, MigrateFn, MigrationSet, MigrationStep, Status, Str};
    use rustc_hash::FxHashMap;

    use super::{Migration, Record, Step, Target, dry_run, follow_renames, run};
    use crate::host::Loader;

    /// A mod's migrations and its map.
    struct Registered {
        owner: usize,
        list: Vec<Migration>,
        map: MigrateFn,
        user: usize,
    }

    /// Taken only for a moment: the maps run without it, so a mod's map may
    /// call the loader.
    static REGISTERED: Mutex<Vec<Arc<Registered>>> = Mutex::new(Vec::new());
    /// The record, read once and written after every run.
    static RECORD: Mutex<Option<Record>> = Mutex::new(None);
    /// Content renames, old GUID to new, once content is sealed.
    static RENAMES: OnceLock<FxHashMap<[u8; 16], [u8; 16]>> = OnceLock::new();

    /// Runs a mod's map; its text stays valid until the mod's next map
    /// call, which only this thread's run makes.
    fn call(
        reg: &Registered,
        migration: usize,
        step: usize,
        input: &str,
    ) -> Result<String, String> {
        let (Ok(migration), Ok(step)) = (u32::try_from(migration), u32::try_from(step)) else {
            return Err("too many steps".to_owned());
        };
        let mut out = Str::EMPTY;
        // SAFETY: the map lives in the mod's DLL, which stays loaded; the
        // input is valid for the call and the safe layer catches panics.
        let status =
            crate::profile::call(reg.owner, crate::profile::Feature::Migrations, || unsafe {
                (reg.map)(
                    core::ptr::with_exposed_provenance_mut(reg.user),
                    migration,
                    step,
                    Str::new(input),
                    &raw mut out,
                )
            });
        // SAFETY: the text stays valid until the mod's next map call.
        let Some(text) = (unsafe { out.as_str() }).map(str::to_owned) else {
            return Err("the result is not UTF-8".to_owned());
        };
        if status.is_ok() {
            Ok(text)
        } else if text.is_empty() {
            Err(format!("refused ({})", status.0))
        } else {
            Err(text)
        }
    }

    /// The mod's migrations, copied from `set`.
    ///
    /// # Safety
    ///
    /// `set` is the mod's valid set, its strings and arrays valid for the
    /// call.
    unsafe fn copy(set: &MigrationSet) -> Option<Vec<Migration>> {
        // SAFETY: forwarded from the caller.
        let migrations = unsafe { slice(set.migrations, set.count) }?;
        migrations
            .iter()
            .map(|m| {
                // SAFETY: as above.
                let steps = unsafe { slice(m.steps, m.count) }?;
                Some(Migration {
                    // SAFETY: as above.
                    id: unsafe { m.id.as_str() }?.to_owned(),
                    steps: steps
                        .iter()
                        .map(|s| {
                            Some(Step {
                                kind: s.kind,
                                // SAFETY: as above.
                                from: unsafe { s.from.as_str() }?.to_owned(),
                                // SAFETY: as above.
                                to: unsafe { s.to.as_str() }?.to_owned(),
                                from_guid: s.from_guid,
                                to_guid: s.to_guid,
                            })
                        })
                        .collect::<Option<_>>()?,
                })
            })
            .collect()
    }

    /// `count` items at `ptr`, empty for none.
    ///
    /// # Safety
    ///
    /// A non-null `ptr` points at `count` valid items.
    unsafe fn slice<'a, T>(ptr: *const T, count: u32) -> Option<&'a [T]> {
        if count == 0 {
            return Some(&[]);
        }
        if ptr.is_null() {
            return None;
        }
        // SAFETY: forwarded from the caller.
        Some(unsafe { core::slice::from_raw_parts(ptr, count as usize) })
    }

    /// Runs `f` on a record that holds only what mod `id` ran on `world`, or
    /// on its settings file for `None`, without the record's lock, so the
    /// mod's map may call the loader; then writes that entry back, unless in
    /// a dry run. `None`, logged once, while the record does not read, so no
    /// migration runs twice.
    fn with_entry<R>(
        loader: &Loader,
        world: Option<&str>,
        id: &str,
        f: impl FnOnce(&mut Record) -> R,
    ) -> Option<R> {
        static REPORTED: std::sync::Once = std::sync::Once::new();
        let mut part = Record::default();
        {
            let mut held = RECORD.lock().unwrap_or_else(PoisonError::into_inner);
            if held.is_none() {
                match Record::read(&loader.data_dir) {
                    Ok(read) => *held = Some(read),
                    Err(e) => {
                        REPORTED.call_once(|| {
                            crate::log!(
                                ERROR,
                                "migrations off: mod-data/{} does not read ({e}); fix or remove it",
                                super::RECORD
                            );
                        });
                        return None;
                    }
                }
            }
            let full = held.as_mut()?;
            part.ran(world, id).clone_from(full.ran(world, id));
        }
        let result = f(&mut part);
        let mut held = RECORD.lock().unwrap_or_else(PoisonError::into_inner);
        let full = held.as_mut()?;
        *full.ran(world, id) = std::mem::take(part.ran(world, id));
        if !dry_run()
            && let Err(e) = full.write(&loader.data_dir)
        {
            crate::log!(WARN, "migrations: the record was not written: {e}");
        }
        Some(result)
    }

    /// Logs a run's outcome under mod `id`.
    fn report(id: &str, place: &str, outcome: &Result<Option<String>, (String, String)>) {
        let line = |level, text: String| crate::log::write(level, id, &text);
        match outcome {
            Ok(None) => {}
            Ok(Some(plan)) if dry_run() => line(
                Level::INFO,
                format!("migrations on {place}, dry run, nothing changed: {plan}"),
            ),
            Ok(Some(plan)) => line(Level::INFO, format!("migrations on {place}: {plan}")),
            Err((plan, reason)) => line(
                Level::ERROR,
                format!("migrations on {place} failed: {reason}; planned: {plan}"),
            ),
        }
    }

    /// Registers mod `index`'s migrations and runs them on its settings file.
    pub(crate) fn register(loader: &Loader, index: usize, set: &MigrationSet) -> Status {
        let Some(record) = loader.mods.get(index) else {
            return Status::BAD_ARGS;
        };
        let taken = |registered: &[Arc<Registered>]| {
            registered.iter().any(|r| r.owner == index) || RENAMES.get().is_some()
        };
        if taken(&REGISTERED.lock().unwrap_or_else(PoisonError::into_inner)) {
            return Status::REFUSED;
        }
        let Some(map) = set.map else {
            return Status::BAD_ARGS;
        };
        // SAFETY: the safe layer passes its valid set for the call.
        let Some(list) = (unsafe { copy(set) }) else {
            return Status::BAD_ARGS;
        };
        let reg = Registered {
            owner: index,
            list,
            map,
            user: set.user.expose_provenance(),
        };
        let path = loader.configs_dir.join(format!("{}.toml", record.id));
        let outcome = with_entry(loader, None, &record.id, |rec| {
            run(
                rec,
                Target::Settings(&path),
                &record.id,
                &reg.list,
                &|m, s, input| call(&reg, m, s, input),
                dry_run(),
            )
        })
        .unwrap_or(Ok(None));
        report(&record.id, "the settings file", &outcome);
        if outcome.is_err() {
            return Status::REFUSED;
        }
        if matches!(outcome, Ok(Some(_))) {
            crate::watch::wrote(&path);
        }
        let mut registered = REGISTERED.lock().unwrap_or_else(PoisonError::into_inner);
        if taken(&registered) {
            return Status::REFUSED;
        }
        registered.push(Arc::new(reg));
        Status::OK
    }

    /// A world loaded: runs every running mod's pending migrations on it,
    /// before the mods read its settings or open its stores. A mod whose
    /// migration fails is turned off.
    pub(crate) fn world_loaded(loader: &Loader, key: &str) {
        let registered = REGISTERED
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        for reg in &registered {
            let Some(record) = loader.mods.get(reg.owner) else {
                continue;
            };
            if record.off.load(std::sync::atomic::Ordering::Acquire) {
                continue;
            }
            let dir = loader.data_dir.join(&record.id).join("worlds").join(key);
            let outcome = with_entry(loader, Some(key), &record.id, |rec| {
                run(
                    rec,
                    Target::World { key, dir: &dir },
                    &record.id,
                    &reg.list,
                    &|m, s, input| call(reg, m, s, input),
                    dry_run(),
                )
            })
            .unwrap_or(Ok(None));
            report(&record.id, &format!("world {key}"), &outcome);
            if let Err((_, reason)) = outcome {
                loader.turn_off(
                    reg.owner,
                    &format!("migration failed on world {key}: {reason}"),
                );
            }
        }
    }

    /// The content renames of every running mod, old GUID to new: sealed on
    /// first call, after every mod is initialised.
    pub(crate) fn renamed(loader: &Loader) -> &'static FxHashMap<[u8; 16], [u8; 16]> {
        RENAMES.get_or_init(|| {
            let registered = REGISTERED.lock().unwrap_or_else(PoisonError::into_inner);
            let direct: FxHashMap<[u8; 16], [u8; 16]> = registered
                .iter()
                .filter(|r| {
                    loader
                        .mods
                        .get(r.owner)
                        .is_some_and(|m| !m.off.load(std::sync::atomic::Ordering::Acquire))
                })
                .flat_map(|r| &r.list)
                .flat_map(|m| &m.steps)
                .filter(|s| s.kind == MigrationStep::RENAME_CONTENT)
                .map(|s| (s.from_guid, s.to_guid))
                .collect();
            follow_renames(&direct)
        })
    }
}

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
mod tests {
    use std::path::PathBuf;

    use enshwick::abi::MigrationStep;

    use super::*;

    #[test]
    fn renames_follow_their_chain_and_a_cycle_ends() {
        let direct: rustc_hash::FxHashMap<[u8; 16], [u8; 16]> = [
            ([1; 16], [2; 16]),
            ([2; 16], [3; 16]),
            ([7; 16], [8; 16]),
            ([8; 16], [7; 16]),
        ]
        .into_iter()
        .collect();
        let followed = follow_renames(&direct);
        assert_eq!(followed[&[1; 16]], [3; 16]);
        assert_eq!(followed[&[2; 16]], [3; 16]);
        assert_eq!(followed[&[7; 16]], [8; 16]);
    }

    fn step(kind: u32, from: &str, to: &str) -> Step {
        Step {
            kind,
            from: from.to_owned(),
            to: to.to_owned(),
            from_guid: [0; 16],
            to_guid: [0; 16],
        }
    }

    fn migration(id: &str, steps: Vec<Step>) -> Migration {
        Migration {
            id: id.to_owned(),
            steps,
        }
    }

    fn double(_: usize, _: usize, input: &str) -> Result<String, String> {
        let n: i64 = input
            .parse()
            .map_err(|_| format!("not a number: {input}"))?;
        Ok((n * 2).to_string())
    }

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("enshwick-migrations-{name}-{}", std::process::id()));
        let _gone = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn settings_steps_keep_comments() {
        let text = "# Cost.\ncost = 3 # each\n# Limit.\nlimit = 4\nold = true\n";
        let m = migration(
            "2",
            vec![
                step(MigrationStep::RENAME_SETTING, "cost", "cost_multiplier"),
                step(MigrationStep::MAP_SETTING, "limit", ""),
                step(MigrationStep::REMOVE_SETTING, "old", ""),
                step(MigrationStep::MAP_SETTING, "missing", ""),
            ],
        );
        let new = apply_settings(text, 0, &m, &double).unwrap();
        assert_eq!(
            new,
            "# Cost.\ncost_multiplier = 3 # each\n# Limit.\nlimit = 8\n"
        );
    }

    #[test]
    fn a_rename_never_overwrites() {
        let m = migration("2", vec![step(MigrationStep::RENAME_SETTING, "a", "b")]);
        let new = apply_settings("a = 1\nb = 2\n", 0, &m, &double).unwrap();
        assert_eq!(new, "a = 1\nb = 2\n");
    }

    #[test]
    fn a_refused_map_names_the_setting() {
        let m = migration("2", vec![step(MigrationStep::MAP_SETTING, "limit", "")]);
        let error = apply_settings("limit = \"x\"\n", 0, &m, &double).unwrap_err();
        assert!(error.starts_with("setting limit: not a number"), "{error}");
    }

    #[test]
    fn the_plan_says_what_will_happen() {
        let all = [
            migration("2", vec![step(MigrationStep::RENAME_SETTING, "a", "b")]),
            migration("3", vec![step(MigrationStep::RENAME_STORE, "x", "y")]),
        ];
        let pending = pending(&all, &["2".to_owned()]);
        assert_eq!(
            plan("safes", &pending, false),
            "safes 3: rename store x to y"
        );
        assert_eq!(plan("safes", &pending, true), "safes 3: nothing here");
    }

    #[test]
    fn a_world_runs_its_pending_migrations_once() {
        let dir = scratch("world");
        let world = dir.join("safes/worlds/w1");
        std::fs::create_dir_all(&world).unwrap();
        std::fs::write(world.join("settings.toml"), "limit = 2\n").unwrap();
        std::fs::write(world.join("owners.toml"), "count = 1\n").unwrap();
        let all = [
            migration("2", vec![step(MigrationStep::MAP_SETTING, "limit", "")]),
            migration(
                "3",
                vec![
                    step(MigrationStep::RENAME_STORE, "owners", "keepers"),
                    step(MigrationStep::MAP_STORE, "keepers", ""),
                ],
            ),
        ];
        let map = |_: usize, step: usize, input: &str| {
            if step == 1 {
                Ok(input.replace('1', "5"))
            } else {
                double(0, 0, input)
            }
        };
        let mut record = Record::default();
        let target = Target::World {
            key: "w1",
            dir: &world,
        };
        let line = run(&mut record, target, "safes", &all, &map, false).unwrap();
        assert_eq!(
            line.as_deref(),
            Some(
                "safes 2: change setting limit; safes 3: rename store owners to keepers, change store keepers"
            )
        );
        assert_eq!(
            std::fs::read_to_string(world.join("settings.toml")).unwrap(),
            "limit = 4\n"
        );
        assert_eq!(
            std::fs::read_to_string(world.join("keepers.toml")).unwrap(),
            "count = 5\n"
        );
        assert_eq!(record.worlds["w1"]["safes"], ["2", "3"]);
        assert_eq!(
            run(&mut record, target, "safes", &all, &map, false).unwrap(),
            None
        );
        assert_eq!(
            std::fs::read_to_string(world.join("settings.toml")).unwrap(),
            "limit = 4\n"
        );
        record.write(&dir).unwrap();
        assert_eq!(Record::read(&dir), Ok(record));
        std::fs::write(dir.join(RECORD), "").unwrap();
        assert!(Record::read(&dir).is_err());
        std::fs::write(dir.join(RECORD), "settings = [").unwrap();
        assert!(Record::read(&dir).is_err());
        std::fs::remove_file(dir.join(RECORD)).unwrap();
        assert_eq!(Record::read(&dir), Ok(Record::default()));
        let _gone = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_migration_that_fails_midway_changes_nothing() {
        let dir = scratch("midway");
        let world = dir.join("safes/worlds/w1");
        std::fs::create_dir_all(&world).unwrap();
        std::fs::write(
            world.join("settings.toml"),
            "limit = 2
",
        )
        .unwrap();
        std::fs::write(
            world.join("owners.toml"),
            "count = 1
",
        )
        .unwrap();
        let all = [migration(
            "2",
            vec![
                step(MigrationStep::MAP_SETTING, "limit", ""),
                step(MigrationStep::RENAME_STORE, "owners", "keepers"),
                step(MigrationStep::MAP_STORE, "keepers", ""),
            ],
        )];
        let target = Target::World {
            key: "w1",
            dir: &world,
        };
        let refuse = |_: usize, step: usize, input: &str| {
            if step == 2 {
                Err("refused".to_owned())
            } else {
                double(0, 0, input)
            }
        };
        let mut record = Record::default();
        let (_, reason) = run(&mut record, target, "safes", &all, &refuse, false).unwrap_err();
        assert!(reason.contains("refused"), "{reason}");
        assert!(record.worlds["w1"]["safes"].is_empty());
        assert_eq!(
            std::fs::read_to_string(world.join("settings.toml")).unwrap(),
            "limit = 2
"
        );
        assert!(world.join("owners.toml").exists() && !world.join("keepers.toml").exists());
        // Run again once the map takes it: whole, once.
        let take = |_: usize, step: usize, input: &str| {
            if step == 2 {
                Ok(input.replace('1', "5"))
            } else {
                double(0, 0, input)
            }
        };
        assert!(run(&mut record, target, "safes", &all, &take, false).is_ok());
        assert_eq!(
            std::fs::read_to_string(world.join("settings.toml")).unwrap(),
            "limit = 4
"
        );
        assert_eq!(
            std::fs::read_to_string(world.join("keepers.toml")).unwrap(),
            "count = 5
"
        );
        assert!(!world.join("owners.toml").exists());
        let _gone = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_world_the_mod_never_saw_starts_current() {
        let dir = scratch("fresh");
        let world = dir.join("safes/worlds/new");
        let all = [migration(
            "2",
            vec![step(MigrationStep::MAP_SETTING, "limit", "")],
        )];
        let mut record = Record::default();
        let target = Target::World {
            key: "new",
            dir: &world,
        };
        assert_eq!(
            run(&mut record, target, "safes", &all, &double, false),
            Ok(None)
        );
        assert_eq!(record.worlds["new"]["safes"], ["2"]);
        let _gone = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_dry_run_changes_and_records_nothing() {
        let dir = scratch("dry");
        let file = dir.join("safes.toml");
        std::fs::write(&file, "limit = 2\n").unwrap();
        let all = [migration(
            "2",
            vec![step(MigrationStep::MAP_SETTING, "limit", "")],
        )];
        let mut record = Record::default();
        let line = run(
            &mut record,
            Target::Settings(&file),
            "safes",
            &all,
            &double,
            true,
        )
        .unwrap();
        assert_eq!(line.as_deref(), Some("safes 2: change setting limit"));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "limit = 2\n");
        assert!(record.settings["safes"].is_empty());
        let _gone = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_failure_records_the_migrations_before_it() {
        let dir = scratch("fail");
        let file = dir.join("safes.toml");
        std::fs::write(&file, "a = 1\nlimit = \"x\"\n").unwrap();
        let all = [
            migration("2", vec![step(MigrationStep::RENAME_SETTING, "a", "b")]),
            migration("3", vec![step(MigrationStep::MAP_SETTING, "limit", "")]),
        ];
        let mut record = Record::default();
        let (_, reason) = run(
            &mut record,
            Target::Settings(&file),
            "safes",
            &all,
            &double,
            false,
        )
        .unwrap_err();
        assert!(reason.starts_with("migration 3: setting limit"), "{reason}");
        assert_eq!(record.settings["safes"], ["2"]);
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "b = 1\nlimit = \"x\"\n"
        );
        let _gone = std::fs::remove_dir_all(&dir);
    }
}
