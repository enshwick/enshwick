//! Settings files watched for changes: a mod's own raise
//! [`Event::SETTINGS_CHANGED`] for it, the loader's re-reads the log levels
//! and raises [`Event::LOADER_SETTINGS_CHANGED`] for every mod.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, SystemTime};

use enshwick_loader_api::abi::Event;

use crate::host::Loader;
use crate::settings::{LOADER_ID, Settings};

/// How often the files are looked at; a few stat calls each time.
const PERIOD: Duration = Duration::from_secs(1);

/// What a settings file looked like when last seen: modified time and length.
type Stamp = Option<(SystemTime, u64)>;

/// Files the loader wrote itself and raised the event for already, with
/// what they looked like after the write.
static WRITTEN: Mutex<Vec<(PathBuf, Stamp)>> = Mutex::new(Vec::new());

/// Notes that the loader wrote `path` and raised its event itself, so the
/// watcher does not raise it again.
pub(crate) fn wrote(path: &Path) {
    let now = stamp(path);
    WRITTEN
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push((path.to_path_buf(), now));
}

/// Whether the loader wrote `path` as it looks now; forgets the write.
fn ours(path: &Path, now: Stamp) -> bool {
    let mut written = WRITTEN.lock().unwrap_or_else(PoisonError::into_inner);
    let found = written.iter().position(|(p, s)| p == path && *s == now);
    found.map(|at| written.swap_remove(at)).is_some()
}

/// A watched file and whom it belongs to: a mod index, or `None` for the loader.
struct Watched {
    owner: Option<usize>,
    path: PathBuf,
    seen: Stamp,
}

fn stamp(path: &Path) -> Stamp {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.modified().ok()?, meta.len()))
}

/// Starts watching on a thread of its own.
pub(crate) fn start(loader: &'static Loader) {
    let mut files: Vec<Watched> = loader
        .mods
        .iter()
        .enumerate()
        .map(|(index, record)| {
            (
                Some(index),
                loader.configs_dir.join(format!("{}.toml", record.id)),
            )
        })
        .chain([(None, loader.configs_dir.join(format!("{LOADER_ID}.toml")))])
        .map(|(owner, path)| Watched {
            owner,
            seen: stamp(&path),
            path,
        })
        .collect();
    let started = std::thread::Builder::new()
        .name("enshwick-settings".into())
        .spawn(move || {
            loop {
                std::thread::sleep(PERIOD);
                for file in &mut files {
                    let now = stamp(&file.path);
                    if now == file.seen {
                        continue;
                    }
                    file.seen = now;
                    if now.is_none() || ours(&file.path, now) {
                        continue;
                    }
                    changed(loader, file.owner);
                }
            }
        });
    if let Err(e) = started {
        crate::log!(WARN, "settings are not watched: {e}");
    }
}

fn changed(loader: &Loader, owner: Option<usize>) {
    if let Some(index) = owner {
        loader.emit(Event::SETTINGS_CHANGED, Some(index), core::ptr::null());
        return;
    }
    let path = loader.configs_dir.join(format!("{LOADER_ID}.toml"));
    match Settings::read(&path) {
        (settings, None) => {
            crate::diag::configure(&settings);
            crate::log::set_levels(settings);
            crate::log!(INFO, "loader settings reloaded");
            loader.emit(Event::LOADER_SETTINGS_CHANGED, None, core::ptr::null());
        }
        (_, Some(problem)) => {
            crate::log!(WARN, "loader settings change skipped: {problem}");
        }
    }
}
