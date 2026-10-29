//! A migration's files, held until every step succeeded, then written all
//! or nothing, with a copy of what they were before.
//!
//! On a world, a commit first copies every file it will change into
//! `.backup/<migration>/` beside them and leaves a note there that it is
//! under way. It writes the files, removes the ones a step removed, and
//! writes the world's record last. On a failure it puts the copies back;
//! after a crash, the next load finds the note and puts them back before
//! anything runs. The copies stay after a commit that succeeded.

use std::io;
use std::path::{Path, PathBuf};

use super::{MigrationError, RecordError};

/// The file in a world folder that lists the migrations that ran on it.
pub const WORLD_RECORD: &str = ".migrations.toml";

/// The folder in a world folder that keeps, per migration, the files it
/// changed as they were before.
pub const BACKUPS: &str = ".backup";

/// The note in a backup that its migration's commit is under way.
const UNDER_WAY: &str = ".under-way.toml";

/// What ran on one world, kept in its folder so it moves and is copied
/// with the world's other files.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct WorldRecord {
    /// The ids of the migrations that ran, in the order they ran.
    pub ran: Vec<String>,
}

impl WorldRecord {
    /// Reads the record of the world folder `dir`; `None` when it has none.
    ///
    /// # Errors
    ///
    /// When the file is there but does not read, does not parse, or is
    /// empty, as a write cut short leaves it.
    pub fn read(dir: &Path) -> Result<Option<Self>, RecordError> {
        let Some(text) = enshwick::files::read_to_string_if_present(&dir.join(WORLD_RECORD))?
        else {
            return Ok(None);
        };
        if text.trim().is_empty() {
            return Err(RecordError::Empty);
        }
        Ok(Some(toml::from_str(&text)?))
    }

    /// Writes the record into the world folder `dir`.
    ///
    /// # Errors
    ///
    /// When the file cannot be written.
    pub fn write(&self, dir: &Path) -> io::Result<()> {
        enshwick::files::write_atomically(&dir.join(WORLD_RECORD), self.text())
    }

    /// The record's file text.
    fn text(&self) -> String {
        let body = toml::to_string(self).unwrap_or_default();
        format!(
            "# Mod migrations that ran on this world; the loader keeps this file.\n\
             # A migration listed here does not run again.\n\n{body}"
        )
    }
}

/// What a backup's note says: the migration, the files copied beside it,
/// and the files the commit makes that were not there.
#[derive(Debug, Default, serde::Deserialize, serde::Serialize)]
#[serde(default)]
struct UnderWay {
    migration: String,
    copied: Vec<String>,
    made: Vec<String>,
}

/// Files a migration changes, held until every step of it succeeded:
/// `None` for a file it removes.
#[derive(Debug, Default)]
pub(super) struct Staged {
    pub(super) files: Vec<(PathBuf, Option<String>)>,
    /// The world's record, written after every other file.
    record: Option<(PathBuf, WorldRecord)>,
}

impl Staged {
    /// The text of `path` as the steps so far leave it; `None` while there
    /// is no such file.
    ///
    /// # Errors
    ///
    /// When the file is there but does not read, such as text that is not
    /// UTF-8 or a file another program holds.
    pub(super) fn read(&self, path: &Path) -> Result<Option<String>, MigrationError> {
        match self.files.iter().rev().find(|(p, _)| p == path) {
            Some((_, text)) => Ok(text.clone()),
            None => enshwick::files::read_to_string_if_present(path).map_err(|e| failed(path, e)),
        }
    }

    /// Whether `path` exists as the steps so far leave it.
    ///
    /// # Errors
    ///
    /// When whether it exists cannot be told.
    pub(super) fn exists(&self, path: &Path) -> Result<bool, MigrationError> {
        match self.files.iter().rev().find(|(p, _)| p == path) {
            Some((_, text)) => Ok(text.is_some()),
            None => path.try_exists().map_err(|e| failed(path, e)),
        }
    }

    pub(super) fn put(&mut self, path: PathBuf, text: Option<String>) {
        self.files.retain(|(p, _)| *p != path);
        self.files.push((path, text));
    }

    /// Records the world folder `dir` as having run `record`'s list, in the
    /// same commit.
    pub(super) fn record(&mut self, dir: &Path, record: WorldRecord) {
        self.record = Some((dir.join(WORLD_RECORD), record));
    }

    /// Writes every staged file, removes the ones staged away, then writes
    /// the record; with `backup`, a copy of each file goes there first and
    /// a failure puts them back.
    ///
    /// # Errors
    ///
    /// The file that could not be copied, written or removed, after the
    /// files are back as they were; with what could not be put back, when
    /// that failed too, and the backup's note stays for the next load.
    pub(super) fn commit(self, backup: Option<(&Path, &str)>) -> Result<(), MigrationError> {
        self.commit_with(backup, &|path, text| {
            enshwick::files::write_atomically(path, text)
        })
    }

    /// [`Staged::commit`] through `write`, which tests make fail.
    pub(super) fn commit_with(
        self,
        backup: Option<(&Path, &str)>,
        write: &dyn Fn(&Path, &str) -> io::Result<()>,
    ) -> Result<(), MigrationError> {
        let Some((folder, migration)) = backup else {
            return self.write_all(write);
        };
        self.copy_to(folder, migration)?;
        match self.write_all(write) {
            Ok(()) => {
                let note = folder.join(UNDER_WAY);
                std::fs::remove_file(&note).map_err(|e| failed(&note, e))
            }
            Err(error) => match restore(folder) {
                Ok(()) => Err(error),
                Err(also) => Err(MigrationError::NotRestored {
                    first: Box::new(error),
                    restore: Box::new(also),
                    backup: folder.display().to_string(),
                }),
            },
        }
    }

    /// Every path the commit writes or removes.
    fn paths(&self) -> impl Iterator<Item = &PathBuf> {
        self.files
            .iter()
            .map(|(path, _)| path)
            .chain(self.record.as_ref().map(|(path, _)| path))
    }

    /// Copies each file the commit touches into `folder`, then leaves the
    /// note that names them; nothing is changed before the note is down.
    fn copy_to(&self, folder: &Path, migration: &str) -> Result<(), MigrationError> {
        match std::fs::remove_dir_all(folder) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(failed(folder, e)),
            _ => {}
        }
        std::fs::create_dir_all(folder).map_err(|e| failed(folder, e))?;
        let mut note = UnderWay {
            migration: migration.to_owned(),
            ..UnderWay::default()
        };
        for path in self.paths() {
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if path.try_exists().map_err(|e| failed(path, e))? {
                std::fs::copy(path, folder.join(name)).map_err(|e| failed(path, e))?;
                note.copied.push(name.to_owned());
            } else {
                note.made.push(name.to_owned());
            }
        }
        let text = toml::to_string(&note).map_err(|e| failed(folder, io::Error::other(e)))?;
        let at = folder.join(UNDER_WAY);
        enshwick::files::write_atomically(&at, text).map_err(|e| failed(&at, e))
    }

    fn write_all(
        self,
        write: &dyn Fn(&Path, &str) -> io::Result<()>,
    ) -> Result<(), MigrationError> {
        for (path, text) in &self.files {
            if let Some(text) = text {
                write(path, text).map_err(|e| failed(path, e))?;
            }
        }
        for (path, text) in &self.files {
            if text.is_none() {
                match std::fs::remove_file(path) {
                    Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(failed(path, e)),
                    _ => {}
                }
            }
        }
        if let Some((path, record)) = &self.record {
            write(path, &record.text()).map_err(|e| failed(path, e))?;
        }
        Ok(())
    }
}

/// Puts back the files a commit under way in the backup `folder` touched,
/// beside the folder's world folder, and removes its note.
///
/// # Errors
///
/// The file that could not be put back; the note stays.
fn restore(folder: &Path) -> Result<(), MigrationError> {
    let note_path = folder.join(UNDER_WAY);
    let note = read_note(&note_path)?.unwrap_or_default();
    let Some(world) = folder.parent().and_then(Path::parent) else {
        return Ok(());
    };
    for name in &note.copied {
        let copy = folder.join(name);
        let bytes = std::fs::read(&copy).map_err(|e| failed(&copy, e))?;
        let path = world.join(name);
        enshwick::files::write_atomically(&path, bytes).map_err(|e| failed(&path, e))?;
    }
    for name in &note.made {
        let path = world.join(name);
        match std::fs::remove_file(&path) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(failed(&path, e)),
            _ => {}
        }
    }
    std::fs::remove_file(&note_path).map_err(|e| failed(&note_path, e))
}

fn read_note(path: &Path) -> Result<Option<UnderWay>, MigrationError> {
    let Some(text) =
        enshwick::files::read_to_string_if_present(path).map_err(|e| failed(path, e))?
    else {
        return Ok(None);
    };
    toml::from_str(&text).map(Some).map_err(|e| {
        failed(
            path,
            io::Error::new(io::ErrorKind::InvalidData, e.message()),
        )
    })
}

/// Finishes what a crash left under way in the world folder `dir`: a
/// commit whose migration `ran` lists only loses its note; any other has
/// its files put back. Returns the migrations put back.
///
/// # Errors
///
/// The file that could not be read or put back.
pub(super) fn recover(dir: &Path, ran: &[String]) -> Result<Vec<String>, MigrationError> {
    let backups = dir.join(BACKUPS);
    let entries = match std::fs::read_dir(&backups) {
        Ok(entries) => entries,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(failed(&backups, e)),
    };
    let mut put_back = Vec::new();
    for entry in entries {
        let folder = entry.map_err(|e| failed(&backups, e))?.path();
        let note_path = folder.join(UNDER_WAY);
        let Some(note) = read_note(&note_path)? else {
            continue;
        };
        if ran.contains(&note.migration) {
            std::fs::remove_file(&note_path).map_err(|e| failed(&note_path, e))?;
        } else {
            restore(&folder)?;
            put_back.push(note.migration);
        }
    }
    Ok(put_back)
}

/// The backup folder of migration `id` in the world folder `dir`: the id
/// with every character a file name may not hold replaced.
#[must_use]
pub(super) fn backup_of(dir: &Path, id: &str) -> PathBuf {
    let name: String = id
        .chars()
        .take(64)
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let name = if name.is_empty() || name.starts_with('.') {
        format!("_{name}")
    } else {
        name
    };
    dir.join(BACKUPS).join(name)
}

/// A file error naming the file.
pub(super) fn failed(path: &Path, reason: io::Error) -> MigrationError {
    MigrationError::File {
        file: path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        reason,
    }
}
