//! Settings files watched for changes: the loader's own re-reads the log
//! levels and raises [`Event::LOADER_SETTINGS_CHANGED`] for every mod. While
//! a world runs, each settings file of each mod in that world is watched,
//! and a change raises [`Event::SETTINGS_CHANGED`] for the mod.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use enshwick::abi::Event;

use crate::host::Loader;
use crate::locks::Relock;
use crate::mod_index::ModIndex;
use crate::settings::{FILE, Settings};

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
    note(&mut WRITTEN.relock(), path, now);
}

/// Writes `path` with `write` and notes the write, holding the list of the
/// loader's writes all the while: a watcher that sees the new file first
/// waits for the note, so the event is raised once.
///
/// # Errors
///
/// What `write` returns; nothing is noted then.
pub(crate) fn write_noted(
    path: &Path,
    write: impl FnOnce() -> std::io::Result<()>,
) -> std::io::Result<()> {
    let mut written = WRITTEN.relock();
    write()?;
    note(&mut written, path, stamp(path));
    Ok(())
}

/// Keeps the latest write of `path` only: an older one can never match
/// again, and a file no watcher looks at must not pile up writes.
fn note(written: &mut Vec<(PathBuf, Stamp)>, path: &Path, now: Stamp) {
    written.retain(|(p, _)| p != path);
    written.push((path.to_path_buf(), now));
}

/// Whether the loader wrote `path` as it looks now; forgets the write.
fn ours(path: &Path, now: Stamp) -> bool {
    let mut written = WRITTEN.relock();
    let found = written.iter().position(|(p, s)| p == path && *s == now);
    found.map(|at| written.swap_remove(at)).is_some()
}

/// A watched file and whom it belongs to: a mod index, or `None` for the loader.
struct Watched {
    owner: Option<usize>,
    path: PathBuf,
    seen: Stamp,
}

/// A mod's settings file of the running world, `owner` its mod index and
/// `file` its name.
struct WorldWatched {
    owner: ModIndex,
    file: String,
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
    let own = loader.mods_dir.with_file_name(FILE);
    let mut files = vec![Watched {
        owner: None,
        seen: stamp(&own),
        path: own,
    }];
    let mut worlds: Vec<WorldWatched> = Vec::new();
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
                // A mod may declare a settings file after the watch started.
                for (owner, record) in loader.mods.iter().enumerate() {
                    for declared in record.settings_files.relock().iter() {
                        let known = worlds
                            .iter()
                            .any(|w| w.owner.get() == owner && w.file == declared.file);
                        if !known {
                            worlds.push(WorldWatched {
                                owner: ModIndex::new(owner),
                                file: declared.file.clone(),
                                key: None,
                                seen: None,
                            });
                        }
                    }
                }
                let key = crate::worlds::current();
                let mut edited = Vec::new();
                for file in &mut worlds {
                    let path = key.as_deref().and_then(|key| {
                        let record = loader.mods.get(file.owner.get())?;
                        Some(crate::worlds::world_file(&record.id, &file.file, key))
                    });
                    let now = path.as_deref().and_then(stamp);
                    if file.changed(key.as_deref(), path.as_deref(), now) {
                        edited.push(file.owner.get());
                    }
                }
                edited.dedup();
                for index in edited {
                    crate::worlds::changed(loader, index);
                }
                crate::recipes::poll(loader);
            }
        });
    if let Err(e) = started {
        crate::log!(WARN, "settings are not watched: {e}");
    }
}

fn changed(loader: &Loader, owner: Option<usize>) {
    if let Some(index) = owner {
        crate::worlds::changed(loader, index);
        return;
    }
    let (settings, problems) = Settings::read(&loader.mods_dir.with_file_name(FILE));
    if problems.lines.is_empty() {
        crate::diag::configure(&settings);
        crate::log::set_levels(settings);
        crate::log!(INFO, "loader settings reloaded");
        loader.emit(Event::LOADER_SETTINGS_CHANGED, None, core::ptr::null());
        return;
    }
    for problem in problems.lines {
        crate::log!(
            ERROR,
            "loader settings change skipped: {problem} ({})",
            crate::errors::BAD_SETTINGS
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_noted_write_is_the_loader_s_own_once() {
        let dir = std::env::temp_dir().join(format!("enshwick-watch-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("you.trading-post.toml");
        write_noted(&path, || {
            std::fs::write(
                &path,
                "limit = 3
",
            )
        })
        .unwrap();
        let now = stamp(&path);
        assert!(ours(&path, now), "the watcher takes it as the loader's");
        assert!(!ours(&path, now), "and only once");
        assert!(
            write_noted(&dir.join("no/such/file"), || Err(
                std::io::ErrorKind::NotFound.into()
            ))
            .is_err()
        );
        let _gone = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn writes_to_one_file_keep_only_the_latest() {
        let mut written = Vec::new();
        let (a, b) = (Path::new("a.toml"), Path::new("b.toml"));
        let stamp = |n| Some((SystemTime::UNIX_EPOCH, n));
        for n in 0..100 {
            note(&mut written, a, stamp(n));
        }
        note(&mut written, b, stamp(1));
        assert_eq!(written.len(), 2);
        assert!(written.contains(&(a.to_path_buf(), stamp(99))));
    }

    #[test]
    fn a_world_s_settings_count_as_changed_only_when_someone_else_edits_them() {
        let path = std::env::temp_dir().join(format!("enshwick-watch-{}", std::process::id()));
        let at = |len| Some((SystemTime::UNIX_EPOCH + Duration::from_secs(len), len));
        let mut file = WorldWatched {
            owner: ModIndex::new(0),
            file: "settings".to_owned(),
            key: None,
            seen: None,
        };
        assert!(!file.changed(None, None, None), "no world runs");
        assert!(!file.changed(Some("a"), Some(&path), at(1)), "just loaded");
        assert!(!file.changed(Some("a"), Some(&path), at(1)), "untouched");
        assert!(file.changed(Some("a"), Some(&path), at(2)), "edited");
        // `enshwick config` writes it and raises the event itself.
        WRITTEN.relock().push((path.clone(), at(3)));
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
