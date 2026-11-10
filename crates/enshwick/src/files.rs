//! Files that keep player data: read so that only a missing file counts as
//! empty, written so that a crash, a power loss or a second writer never
//! leaves half a file.
//!
//! A caller that starts empty on `Ok(None)` from [`read_if_present`] or
//! [`read_to_string_if_present`] never writes over a file it could not read:
//! a lock held by a backup tool or a permission error is an `Err`, not an
//! absent file. [`write_atomically`] writes beside the file, flushes it to
//! disk and renames it over the old one, which stays as [`backup_path`].
//!
//! # Examples
//!
//! ```
//! use enshwick::files;
//!
//! let path = std::env::temp_dir().join("enshwick-files-doc/progress.toml");
//! files::write_atomically(&path, "level = 2\n")?;
//! let text = files::read_to_string_if_present(&path)?;
//! assert_eq!(text.as_deref(), Some("level = 2\n"));
//! # Ok::<(), std::io::Error>(())
//! ```

use std::io;
use std::path::{Path, PathBuf};

/// The contents of `path`, or `None` when no file is there.
///
/// # Errors
///
/// Every error but [`io::ErrorKind::NotFound`]: the file is there and may
/// hold data, so it must not be written over.
///
/// # Examples
///
/// ```
/// let path = std::env::temp_dir().join("enshwick-files-doc-none.bin");
/// assert_eq!(enshwick::files::read_if_present(&path)?, None);
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn read_if_present(path: &Path) -> io::Result<Option<Vec<u8>>> {
    absent_as_none(std::fs::read(path))
}

/// The text of `path`, or `None` when no file is there.
///
/// # Errors
///
/// Every error but [`io::ErrorKind::NotFound`], text that is not UTF-8
/// included.
///
/// # Examples
///
/// See [the module](self).
pub fn read_to_string_if_present(path: &Path) -> io::Result<Option<String>> {
    absent_as_none(std::fs::read_to_string(path))
}

fn absent_as_none<T>(read: io::Result<T>) -> io::Result<Option<T>> {
    match read {
        Ok(contents) => Ok(Some(contents)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

/// Where [`write_atomically`] keeps the file it replaced: `<path>.bak`.
///
/// # Examples
///
/// ```
/// use std::path::Path;
///
/// let backup = enshwick::files::backup_path(Path::new("worlds/w/owners.toml"));
/// assert_eq!(backup, Path::new("worlds/w/owners.toml.bak"));
/// ```
#[must_use]
pub fn backup_path(path: &Path) -> PathBuf {
    with_suffix(path, BACKUP_SUFFIX)
}

/// The suffix of [`backup_path`].
const BACKUP_SUFFIX: &str = ".bak";

/// How many random temporary names [`write_atomically`] tries before it
/// gives up; one taken name in 2^64 is already a stale file of a crash.
const TEMPORARY_TRIES: usize = 4;

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

/// Writes `contents` to `path` whole, keeping the old file as
/// [`backup_path`], so a crash leaves either the old file or the new one.
///
/// The text goes to a temporary file of its own beside `path`, is flushed
/// to disk and renamed over the old file; two writers at once never share
/// the temporary file. The folder is made when missing.
///
/// # Errors
///
/// When the folder, the temporary file, the backup or the rename fails; the
/// old file then stays as it was and no temporary file is left. A backup or
/// rename refused for access, as while another program or writer briefly
/// holds the file, is tried again for up to about a second per step first.
///
/// # Examples
///
/// See [the module](self).
pub fn write_atomically(path: &Path, contents: impl AsRef<[u8]>) -> io::Result<()> {
    if let Some(dir) = path.parent().filter(|dir| !dir.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)?;
    }
    let (temporary, mut file) = create_temporary(path)?;
    // On disk before the rename, so a power loss never leaves the new name
    // on an empty file.
    let written = io::Write::write_all(&mut file, contents.as_ref()).and_then(|()| file.sync_all());
    drop(file);
    let placed = written
        .and_then(|()| keep_backup(path))
        .and_then(|()| while_busy(|| std::fs::rename(&temporary, path)));
    if placed.is_err() {
        let _gone = while_busy(|| std::fs::remove_file(&temporary));
    }
    placed
}

/// How often [`while_busy`] tries a step: about a second of waiting per step,
/// since on a machine under load a writer that holds the file may not run
/// again for tens of milliseconds.
const BUSY_TRIES: u32 = 32;

/// Windows' `ERROR_SHARING_VIOLATION` and `ERROR_LOCK_VIOLATION`, which
/// `io::ErrorKind` leaves uncategorized.
const BUSY_CODES: [i32; 2] = [32, 33];

/// Whether `error` says another handle holds the file for now.
fn busy(error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::PermissionDenied
        || error
            .raw_os_error()
            .is_some_and(|code| BUSY_CODES.contains(&code))
}

/// `step`, tried again after a short wait while it is refused for access:
/// Windows refuses to delete, link or rename over a file that another handle
/// holds open without delete sharing, as a virus scanner, an indexer or a
/// second writer's backup copy briefly does.
fn while_busy<T>(mut step: impl FnMut() -> io::Result<T>) -> io::Result<T> {
    let mut tries = 1;
    loop {
        match step() {
            Err(e) if busy(&e) && tries < BUSY_TRIES => {
                std::thread::sleep(std::time::Duration::from_millis(u64::from(tries) * 2));
                tries += 1;
            }
            done => return done,
        }
    }
}

/// A new file beside `path` that no other writer has, named by this
/// process and a random number.
fn create_temporary(path: &Path) -> io::Result<(PathBuf, std::fs::File)> {
    for _ in 0..TEMPORARY_TRIES {
        let suffix = format!(".{}-{:016x}.tmp", std::process::id(), fastrand::u64(..));
        let temporary = with_suffix(path, &suffix);
        match std::fs::File::create_new(&temporary) {
            Ok(file) => return Ok((temporary, file)),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        format!("no free temporary name beside {}", path.display()),
    ))
}

/// Keeps the file at `path` as its [`backup_path`]: a second link to it,
/// or a copy where the file system has no links. Nothing to keep while no
/// file is there.
fn keep_backup(path: &Path) -> io::Result<()> {
    keep_backup_with(path, |from, to| std::fs::hard_link(from, to))
}

/// [`keep_backup`] with `link` as the step that makes the second link.
fn keep_backup_with(
    path: &Path,
    mut link: impl FnMut(&Path, &Path) -> io::Result<()>,
) -> io::Result<()> {
    let backup = backup_path(path);
    match while_busy(|| std::fs::remove_file(&backup)) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
        _ => {}
    }
    // Bounded: a file system that answers not found for a file it still
    // lists must not hang the writer; the earlier whole file then stays.
    for _ in 0..BUSY_TRIES {
        match while_busy(|| link(path, &backup)) {
            // A writer at the same time kept it a moment ago: a whole
            // earlier file stays, and a copy would hold the file open
            // against the other writers' renames.
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => return Ok(()),
            // Linux also answers so for a file that another writer's rename
            // replaced between the lookup of its name and the link (the
            // `i_nlink == 0` check of `vfs_link`); the new file is there to
            // keep, and each further turn needs one more whole write to land
            // in that gap.
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                if matches!(path.try_exists(), Ok(false)) {
                    return Ok(());
                }
            }
            Ok(()) => return Ok(()),
            // Still refused once the tries ran out, as a file system without
            // links answers.
            Err(_) => return std::fs::copy(path, &backup).map(drop),
        }
    }
    Ok(())
}

/// The functions of [`files`](self) a script calls, as its namespace
/// `files`: a script writes files, such as exports for a wiki or a map, and
/// reads only its own folder.
#[doc(hidden)]
#[derive(Clone, Copy, Debug)]
pub struct FilesFns;

#[crate::script(files_fns)]
#[expect(
    clippy::unused_self,
    clippy::needless_pass_by_value,
    reason = "a script call takes its namespace and hands its values over as its own copies"
)]
impl FilesFns {
    /// Writes `contents` to `path` whole, making its folders, as
    /// [`write_atomically`] writes.
    ///
    /// # Errors
    ///
    /// As [`write_atomically`].
    #[script]
    pub fn write_atomically(
        self,
        path: PathBuf,
        contents: crate::resources::Bytes,
    ) -> io::Result<()> {
        write_atomically(&path, contents.0)
    }

    /// Where [`write_atomically`] keeps the file it replaced, as
    /// [`backup_path`] names it.
    #[script]
    #[must_use]
    pub fn backup_path(self, path: PathBuf) -> PathBuf {
        backup_path(&path)
    }
}

impl crate::script::Bound for FilesFns {
    const NAME: &'static str = "files";
    const DOC: &'static str =
        "Files a mod writes whole, such as exports for tools outside the game.";
    const PARTS: &'static [&'static [crate::script::Function<Self>]] = &[Self::SCRIPT_FILES_FNS];

    fn resolve(_load: &mut crate::Load<'_>) -> Result<Self, crate::Off> {
        Ok(Self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("enshwick-files-{name}-{}", std::process::id()));
        let _gone = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn a_missing_file_reads_as_none() {
        let dir = dir("missing");
        assert_eq!(read_if_present(&dir.join("absent.bin")).unwrap(), None);
        assert_eq!(
            read_to_string_if_present(&dir.join("absent.toml")).unwrap(),
            None
        );
        let _gone = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_file_that_does_not_read_is_an_error_never_absent() {
        let dir = dir("unreadable");
        let folder = dir.join("data.bin");
        std::fs::create_dir(&folder).unwrap();
        assert!(
            read_if_present(&folder).is_err(),
            "a folder in the file's place"
        );
        let text = dir.join("settings.toml");
        std::fs::write(&text, [0xff, 0xfe, 0x00]).unwrap();
        assert!(read_to_string_if_present(&text).is_err(), "not UTF-8");
        assert_eq!(
            read_if_present(&text).unwrap().as_deref(),
            Some(&[0xff, 0xfe, 0][..])
        );
        let _gone = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_write_keeps_the_previous_file_as_its_backup() {
        let dir = dir("backup");
        let path = dir.join("worlds/w/owners.toml");
        write_atomically(&path, "first").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "first");
        assert!(!backup_path(&path).exists(), "nothing replaced yet");
        write_atomically(&path, "second").unwrap();
        write_atomically(&path, b"third").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "third");
        assert_eq!(
            std::fs::read_to_string(backup_path(&path)).unwrap(),
            "second"
        );
        assert_eq!(
            names(&dir.join("worlds/w")),
            ["owners.toml", "owners.toml.bak"],
            "no temporary file left"
        );
        let _gone = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_failed_write_leaves_the_old_file_and_no_temporary_one() {
        let dir = dir("failed");
        let path = dir.join("attachments.bin");
        std::fs::create_dir(&path).unwrap();
        std::fs::write(path.join("inside"), "kept").unwrap();
        assert!(write_atomically(&path, "new").is_err());
        assert_eq!(
            std::fs::read_to_string(path.join("inside")).unwrap(),
            "kept"
        );
        assert_eq!(names(&dir), ["attachments.bin"]);
        let _gone = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn writers_at_once_never_share_a_temporary_file() {
        let dir = dir("writers");
        let path = dir.join("settings.toml");
        let texts: Vec<String> = (0..8)
            .map(|n| format!("value = {n}\n").repeat(2000))
            .collect();
        // Rounds, since a lost race shows only now and then.
        for _ in 0..4 {
            std::thread::scope(|scope| {
                for text in &texts {
                    let path = &path;
                    scope.spawn(move || {
                        let refused: Vec<String> = (0..20)
                            .filter_map(|_| write_atomically(path, text).err())
                            .map(|e| format!("{:?}: {e}", e.kind()))
                            .collect();
                        assert!(refused.is_empty(), "{refused:#?}");
                    });
                }
            });
            let last = std::fs::read_to_string(&path).unwrap();
            assert!(texts.contains(&last), "one writer's whole text");
            assert_eq!(names(&dir), ["settings.toml", "settings.toml.bak"]);
        }
        let _gone = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_file_replaced_while_its_backup_is_linked_is_linked_again() {
        let dir = dir("replaced");
        let path = dir.join("settings.toml");
        std::fs::write(&path, "kept").unwrap();
        let mut links = 0;
        keep_backup_with(&path, |from, to| {
            links += 1;
            if links == 1 {
                return Err(io::ErrorKind::NotFound.into());
            }
            std::fs::hard_link(from, to)
        })
        .unwrap();
        assert_eq!(std::fs::read_to_string(backup_path(&path)).unwrap(), "kept");
        let absent = dir.join("absent.toml");
        keep_backup(&absent).unwrap();
        assert_eq!(names(&dir), ["settings.toml", "settings.toml.bak"]);
        let _gone = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_link_that_never_finds_a_file_still_there_gives_up() {
        let dir = dir("never-found");
        let path = dir.join("settings.toml");
        std::fs::write(&path, "kept").unwrap();
        let mut links = 0;
        keep_backup_with(&path, |_, _| {
            links += 1;
            Err(io::ErrorKind::NotFound.into())
        })
        .unwrap();
        assert_eq!(links, BUSY_TRIES);
        assert_eq!(names(&dir), ["settings.toml"]);
        let _gone = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_step_refused_for_access_is_tried_again_and_any_other_error_is_not() {
        let refused = || io::Error::from(io::ErrorKind::PermissionDenied);
        let mut calls = 0;
        let placed = while_busy(|| {
            calls += 1;
            if calls < 3 { Err(refused()) } else { Ok(calls) }
        });
        assert_eq!(placed.unwrap(), 3);
        let mut calls = 0;
        let missing = while_busy::<()>(|| {
            calls += 1;
            Err(io::ErrorKind::NotFound.into())
        });
        assert_eq!(
            (missing.unwrap_err().kind(), calls),
            (io::ErrorKind::NotFound, 1)
        );
        let mut calls = 0;
        let held = while_busy::<()>(|| {
            calls += 1;
            Err(refused())
        });
        assert_eq!(
            (held.unwrap_err().kind(), calls),
            (io::ErrorKind::PermissionDenied, BUSY_TRIES)
        );
    }
}
