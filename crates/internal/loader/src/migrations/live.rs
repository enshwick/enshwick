use std::sync::{Arc, Mutex, OnceLock};

use enshwick::abi::{Level, MigrateFn, MigrationSet, MigrationStep, Status, Str};
use rustc_hash::FxHashMap;

use crate::host::Loader;
use crate::locks::Relock;
use crate::mod_index::ModIndex;

use super::{MapError, Migration, Record, Step, Target, dry_run, follow_renames, run};

/// A mod's migrations and its map.
struct Registered {
    owner: ModIndex,
    list: Vec<Migration>,
    map: MigrateFn,
    user: usize,
}

/// Taken only for a moment: the maps run without it, so a mod's map may
/// call the loader.
static REGISTERED: Mutex<Vec<Arc<Registered>>> = Mutex::new(Vec::new());
/// The record, read once and written after every run.
static RECORD: Mutex<Option<Record>> = Mutex::new(None);
/// The mods whose settings file the loader wrote from their defaults in
/// this process, before they registered their migrations.
static BORN: Mutex<Vec<usize>> = Mutex::new(Vec::new());
/// Content renames, old GUID to new, once content is sealed.
static RENAMES: OnceLock<FxHashMap<[u8; 16], [u8; 16]>> = OnceLock::new();

/// Runs a mod's map; its text stays valid until the mod's next map
/// call, which only this thread's run makes.
fn call(reg: &Registered, migration: usize, step: usize, input: &str) -> Result<String, MapError> {
    let (Ok(migration), Ok(step)) = (u32::try_from(migration), u32::try_from(step)) else {
        return Err(MapError::TooManySteps);
    };
    let mut out = Str::EMPTY;
    // SAFETY: `reg.map` is the mod's migration function in its DLL, which
    // stays loaded for the process, called with the `user` pointer it was
    // registered with; `input` and `out` are locals alive for the call, and
    // the safe layer catches the map's panics.
    let status = crate::profile::call(reg.owner, crate::profile::Feature::Migrations, || unsafe {
        (reg.map)(
            core::ptr::with_exposed_provenance_mut(reg.user),
            migration,
            step,
            Str::new(input),
            &raw mut out,
        )
    });
    // SAFETY: `out` is `Str::EMPTY` or the text the map wrote, which the ABI
    // keeps valid and unchanged until the mod's next map call; it is copied
    // here.
    let Some(text) = (unsafe { out.as_str() }).map(str::to_owned) else {
        return Err(MapError::NotUtf8);
    };
    if status.is_ok() {
        Ok(text)
    } else if text.is_empty() {
        Err(MapError::Refused(status.0))
    } else {
        Err(MapError::Said(text))
    }
}

/// The mod's migrations, copied from `set`.
///
/// # Safety
///
/// - `set.migrations` is null or points at `set.count` aligned
///   migrations, and each migration's `steps` at its `count` aligned
///   steps, all readable and unchanged for the call.
/// - Every migration id and step text is readable for its length and
///   unchanged for the call.
unsafe fn copy(set: &MigrationSet) -> Option<Vec<Migration>> {
    // SAFETY: the caller guarantees `set.migrations` null or pointing at
    // `set.count` aligned migrations, readable and unchanged for the call.
    let migrations = unsafe { slice(set.migrations, set.count) }?;
    migrations
        .iter()
        .map(|m| {
            // SAFETY: the caller guarantees each migration's `steps` null or
            // pointing at its `count` aligned steps, readable and unchanged.
            let steps = unsafe { slice(m.steps, m.count) }?;
            Some(Migration {
                // SAFETY: the caller guarantees every migration id readable
                // and unchanged for the call; it is copied here.
                id: unsafe { m.id.as_str() }?.to_owned(),
                steps: steps
                    .iter()
                    .map(|s| {
                        Some(Step {
                            kind: s.kind,
                            // SAFETY: the caller guarantees every step text
                            // readable and unchanged for the call; it is
                            // copied here.
                            from: unsafe { s.from.as_str() }?.to_owned(),
                            // SAFETY: the caller guarantees every step text
                            // readable and unchanged for the call; it is
                            // copied here.
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
/// - A non-null `ptr` with a nonzero `count` points at `count`
///   initialised, aligned items.
/// - Those items stay alive and unchanged for the lifetime `'a` the
///   caller picks.
unsafe fn slice<'a, T>(ptr: *const T, count: u32) -> Option<&'a [T]> {
    if count == 0 {
        return Some(&[]);
    }
    if ptr.is_null() {
        return None;
    }
    // SAFETY: `ptr` is non-null and `count` nonzero, checked just before;
    // the caller guarantees `count` aligned items there, alive and
    // unchanged for `'a`.
    Some(unsafe { core::slice::from_raw_parts(ptr, count as usize) })
}

/// Runs `f` on a record that holds only what mod `id` ran on `world`, or
/// on its settings file for `None`, without the record's lock, so the
/// mod's map may call the loader; then writes that entry back when it
/// changed, unless in a dry run.
///
/// While the record does not read, logged once, `f` runs on an empty
/// record only where the world `stands_alone` on a record of its own;
/// elsewhere the reason is returned, so no migration runs twice.
fn with_entry<R>(
    loader: &Loader,
    world: Option<&str>,
    id: &str,
    stands_alone: bool,
    f: impl FnOnce(&mut Record) -> R,
) -> Result<R, ReadRecordError> {
    static REPORTED: std::sync::Once = std::sync::Once::new();
    let mut part = Record::default();
    {
        let mut held = RECORD.relock();
        if held.is_none() {
            match Record::read(&loader.data_dir) {
                Ok(read) => *held = Some(read),
                Err(source) => {
                    let reason = ReadRecordError { source };
                    REPORTED.call_once(|| {
                        crate::log!(
                            ERROR,
                            "{reason}: mods whose migrations need it stay off; fix or remove it"
                        );
                    });
                    if !stands_alone {
                        return Err(reason);
                    }
                }
            }
        }
        let Some(full) = held.as_ref() else {
            drop(held);
            return Ok(f(&mut part));
        };
        part.set_entry(world, id, full.entry(world, id).cloned());
    }
    let result = f(&mut part);
    let mut held = RECORD.relock();
    if let Some(full) = held.as_mut()
        && full.set_entry(world, id, part.entry(world, id).cloned())
        && !dry_run()
        && let Err(e) = full.write(&loader.data_dir)
    {
        crate::log!(WARN, "migrations: the record was not written: {e}");
    }
    Ok(result)
}

/// Why the shared record of migrations that ran does not read.
#[derive(Debug, thiserror::Error)]
#[error("mod-data/{} does not read ({source})", super::RECORD)]
struct ReadRecordError {
    #[source]
    source: super::RecordError,
}

/// Why a mod's migrations did not run through on a place: the run stopped,
/// or the shared record they are counted in does not read.
#[derive(Debug, thiserror::Error)]
enum RunOnError {
    #[error(transparent)]
    Run(#[from] super::RunMigrationsError),
    #[error(transparent)]
    Record(#[from] ReadRecordError),
}

impl RunOnError {
    /// The plan line of what was pending; empty when nothing ran.
    fn plan(&self) -> &str {
        match self {
            Self::Run(stopped) => stopped.plan(),
            Self::Record(_) => "",
        }
    }
}

/// Logs a run's outcome under mod `id`.
fn report(id: &str, place: &str, outcome: &Result<Option<String>, RunOnError>) {
    let line = |level, text: String| crate::log::write(level, id, &text);
    match outcome {
        Ok(None) => {}
        Ok(Some(plan)) if dry_run() => line(
            Level::INFO,
            format!("migrations on {place}, dry run, nothing changed: {plan}"),
        ),
        Ok(Some(plan)) => line(Level::INFO, format!("migrations on {place}: {plan}")),
        Err(stopped) if stopped.plan().is_empty() => line(
            Level::ERROR,
            format!("migrations on {place} did not run: {stopped}"),
        ),
        Err(stopped) => line(
            Level::ERROR,
            format!(
                "migrations on {place} failed: {stopped}; planned: {}",
                stopped.plan()
            ),
        ),
    }
}

/// Registers mod `index`'s migrations and runs them on its settings file.
pub(crate) fn register(loader: &Loader, index: usize, set: &MigrationSet) -> Status {
    if loader.mods.get(index).is_none() {
        return Status::BAD_ARGS;
    }
    let taken = |registered: &[Arc<Registered>]| {
        registered.iter().any(|r| r.owner == ModIndex::new(index)) || RENAMES.get().is_some()
    };
    if taken(&REGISTERED.relock()) {
        return Status::REFUSED;
    }
    let Some(map) = set.map else {
        return Status::BAD_ARGS;
    };
    // SAFETY: the only caller, the `migrate` entry of the host table, passes
    // the set a mod handed over in that call, whose arrays and texts the ABI
    // has the mod keep readable and unchanged for the call.
    let Some(list) = (unsafe { copy(set) }) else {
        return Status::BAD_ARGS;
    };
    let reg = Registered {
        owner: ModIndex::new(index),
        list,
        map,
        user: set.user.expose_provenance(),
    };
    let born = BORN.relock().contains(&index);
    if !run_settings(loader, &reg, born) {
        return Status::REFUSED;
    }
    let mut registered = REGISTERED.relock();
    if taken(&registered) {
        return Status::REFUSED;
    }
    registered.push(Arc::new(reg));
    Status::OK
}

/// Runs `reg`'s pending migrations on its mod's settings file; whether
/// they ran without a failure.
fn run_settings(loader: &Loader, reg: &Registered, born: bool) -> bool {
    let Some(record) = loader.mods.get(reg.owner.get()) else {
        return false;
    };
    let path = loader.configs_dir.join(format!("{}.toml", record.id));
    let outcome = with_entry(loader, None, &record.id, false, |rec| {
        run(
            rec,
            Target::Settings { path: &path, born },
            &record.id,
            &reg.list,
            &|m, s, input| call(reg, m, s, input),
            dry_run(),
        )
    })
    .map_err(RunOnError::from)
    .and_then(|ran| ran.map_err(RunOnError::from));
    report(&record.id, "the settings file", &outcome);
    if matches!(outcome, Ok(Some(_))) {
        crate::watch::wrote(&path);
    }
    outcome.is_ok()
}

/// The loader wrote mod `index`'s settings file from its defaults: the
/// mod has never seen it, so its migrations start current there, and
/// the settings it takes from other mods come in. A failure is logged,
/// and the mod reads its defaults.
pub(crate) fn settings_written(loader: &Loader, index: usize) {
    let reg = REGISTERED
        .relock()
        .iter()
        .find(|r| r.owner == ModIndex::new(index))
        .cloned();
    match reg {
        Some(reg) => {
            let _logged = run_settings(loader, &reg, true);
        }
        None => BORN.relock().push(index),
    }
}

/// A world loaded: runs every running mod's pending migrations on it,
/// before the mods read its settings or open its stores. A mod whose
/// migration fails is turned off.
pub(crate) fn world_loaded(loader: &Loader, key: &str) {
    let registered = REGISTERED.relock().clone();
    for reg in &registered {
        let Some(record) = loader.mods.get(reg.owner.get()) else {
            continue;
        };
        if record.off.load(std::sync::atomic::Ordering::Acquire) {
            continue;
        }
        let dir = loader.data_dir.join(&record.id).join("worlds").join(key);
        let stands_alone = matches!(super::WorldRecord::read(&dir), Ok(Some(_)));
        let outcome = with_entry(loader, Some(key), &record.id, stands_alone, |rec| {
            run(
                rec,
                Target::World { key, dir: &dir },
                &record.id,
                &reg.list,
                &|m, s, input| call(reg, m, s, input),
                dry_run(),
            )
        })
        .map_err(RunOnError::from)
        .and_then(|ran| ran.map_err(RunOnError::from));
        report(&record.id, &format!("world {key}"), &outcome);
        if let Err(reason) = outcome {
            loader.turn_off(
                reg.owner,
                &format!("migration failed on world {key}: {reason}"),
            );
        }
    }
}

/// The world `from` arrived under the key `to`, its mods' data moved or
/// `copied` there: what the shared record kept for `from` counts for `to`.
/// A record that does not read is left alone; its error shows when the
/// world's migrations run.
pub(crate) fn world_arrived(loader: &Loader, from: &str, to: &str, copied: bool) {
    let mut held = RECORD.relock();
    if held.is_none() {
        let Ok(read) = Record::read(&loader.data_dir) else {
            return;
        };
        *held = Some(read);
    }
    if let Some(full) = held.as_mut()
        && full.world_arrived(from, to, copied)
        && !dry_run()
        && let Err(e) = full.write(&loader.data_dir)
    {
        crate::log!(WARN, "migrations: the record was not written: {e}");
    }
}

/// The content renames of every running mod, old GUID to new: sealed on
/// first call, after every mod is initialised.
pub(crate) fn renamed(loader: &Loader) -> &'static FxHashMap<[u8; 16], [u8; 16]> {
    RENAMES.get_or_init(|| {
        let registered = REGISTERED.relock();
        let direct: FxHashMap<[u8; 16], [u8; 16]> = registered
            .iter()
            .filter(|r| {
                loader
                    .mods
                    .get(r.owner.get())
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
