//! Settings files watched for changes: a mod's own raise
//! [`Event::SETTINGS_CHANGED`] for it, the loader's re-reads the log levels
//! and raises [`Event::LOADER_SETTINGS_CHANGED`] for every mod. While a world
//! runs, each mod's settings of that world are watched too, since they are
//! what the mod reads.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, SystemTime};

use enshwick_loader_api::abi::{Event, Level, Side};
use enshwick_loader_api::config::Stage;

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

/// A mod's settings file of the running world, `owner` its mod index.
struct WorldWatched {
    owner: usize,
    key: Option<String>,
    seen: Stamp,
}

impl WorldWatched {
    /// Whether the file at `path` of the world `key`, looking `now`, changed
    /// since the last look by someone other than the loader. A world just
    /// loaded is no change: its load raised the event already.
    fn changed(&mut self, key: Option<&str>, path: Option<&Path>, now: Stamp) -> bool {
        let (Some(key), Some(path)) = (key, path) else {
            self.key = None;
            return false;
        };
        if self.key.as_deref() != Some(key) {
            self.key = Some(key.to_owned());
            self.seen = now;
            let _loaded = ours(path, now);
            return false;
        }
        if now == self.seen {
            return false;
        }
        self.seen = now;
        now.is_some() && !ours(path, now)
    }
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
    let mut worlds: Vec<WorldWatched> = (0..loader.mods.len())
        .map(|owner| WorldWatched {
            owner,
            key: None,
            seen: None,
        })
        .collect();
    // A pack's data files; the dedicated server applies what it can of a
    // change at once.
    let mut packs: Vec<(usize, PathBuf, Stamp)> = if loader.side == Side::SERVER {
        crate::packs::watched(loader)
            .into_iter()
            .map(|(index, path)| {
                let seen = stamp(&path);
                (index, path, seen)
            })
            .collect()
    } else {
        Vec::new()
    };
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
                    if let Some(index) = file.owner {
                        note_shadowed(loader, index);
                    }
                }
                let key = crate::worlds::current();
                for file in &mut worlds {
                    let path = key.as_deref().and_then(|key| {
                        let record = loader.mods.get(file.owner)?;
                        Some(crate::worlds::world_file(&loader.data_dir, &record.id, key))
                    });
                    let now = path.as_deref().and_then(stamp);
                    if file.changed(key.as_deref(), path.as_deref(), now) {
                        changed(loader, Some(file.owner));
                    }
                }
                let mut touched: Vec<usize> = Vec::new();
                for (index, path, seen) in &mut packs {
                    let now = stamp(path);
                    if now != *seen {
                        *seen = now;
                        if !touched.contains(index) {
                            touched.push(*index);
                        }
                    }
                }
                for index in touched {
                    crate::packs::changed(loader, index);
                }
            }
        });
    if let Err(e) = started {
        crate::log!(WARN, "settings are not watched: {e}");
    }
}

/// Tells the mod's log, when its settings file changed while a world runs,
/// that the world's own settings stay in force and where they are.
fn note_shadowed(loader: &Loader, index: usize) {
    let (Some(key), Some(record)) = (crate::worlds::current(), loader.mods.get(index)) else {
        return;
    };
    let per_world = record
        .schema
        .lock()
        .ok()
        .and_then(|schema| schema.clone())
        .is_some_and(|schema| schema.fields.iter().any(|f| f.stage != Stage::Client));
    if !per_world {
        return;
    }
    let world = crate::worlds::world_file(&loader.data_dir, &record.id, &key);
    crate::log::write(
        Level::INFO,
        &record.id,
        &format!(
            "{}.toml changed while world {key} runs: its own player settings apply now; \
             this world reads the rest from {}, changed there, with /config or in the world \
             settings; the file is the template for new worlds",
            record.id,
            world.display()
        ),
    );
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_world_s_settings_count_as_changed_only_when_someone_else_edits_them() {
        let path = std::env::temp_dir().join(format!("enshwick-watch-{}", std::process::id()));
        let at = |len| Some((SystemTime::UNIX_EPOCH + Duration::from_secs(len), len));
        let mut file = WorldWatched {
            owner: 0,
            key: None,
            seen: None,
        };
        assert!(!file.changed(None, None, None), "no world runs");
        assert!(!file.changed(Some("a"), Some(&path), at(1)), "just loaded");
        assert!(!file.changed(Some("a"), Some(&path), at(1)), "untouched");
        assert!(file.changed(Some("a"), Some(&path), at(2)), "edited");
        // `/config` writes it and raises the event itself.
        WRITTEN
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((path.clone(), at(3)));
        assert!(
            !file.changed(Some("a"), Some(&path), at(3)),
            "the loader's own"
        );
        assert!(
            !file.changed(Some("b"), Some(&path), at(4)),
            "another world"
        );
        assert!(file.changed(Some("b"), Some(&path), at(5)));
        assert!(!file.changed(Some("b"), Some(&path), None), "deleted");
        assert!(!file.changed(None, None, None));
        assert!(!file.changed(Some("b"), Some(&path), at(6)), "loaded again");
    }
}
