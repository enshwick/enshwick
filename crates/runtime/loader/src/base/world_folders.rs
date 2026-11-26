//! The two folders beside a world's save, and the check that they belong to
//! the world in that save.
//!
//! A world's mod settings live in `<stem>-mod-settings/` and the data mods
//! write for it in `<stem>-mod-data/`, beside `<stem>.ksd`. The stem is a
//! slot the game gives the next new world once a world is deleted, and the
//! game leaves the folders behind (client 26320373). So the loader records
//! the world's GUID in `<stem>-mod-data/enshwick/world.toml` and, when a
//! save in the slot holds another GUID or is gone, renames both folders to
//! `<name>.old-<date>-<time>` (`-2` on when that name is taken), so the new
//! world starts from the mods' defaults and nothing is deleted. A rename
//! that fails renames back the folder renamed before it, so a world never
//! reads one folder of its own and one of another world's.
//!
//! Every `.ksd` starts with a 0x30-byte `SV2C` header whose bytes 0x0C to
//! 0x1C are the world's random GUID, the same across saves and new for each
//! world (server 25988414, client 426).

use std::fmt::Write as _;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

/// The suffix of a world's settings folder.
pub const SETTINGS_SUFFIX: &str = "-mod-settings";
/// The suffix of a world's data folder.
pub const DATA_SUFFIX: &str = "-mod-data";
/// The file in the loader's data folder of a world that names its GUID.
pub const RECORD_FILE: &str = "world.toml";

/// The extension of a world's save.
const SAVE_EXTENSION: &str = "ksd";
/// The first bytes of a save.
const MAGIC: &[u8; 4] = b"SV2C";
/// Where a save's header holds the world's GUID.
const GUID_AT: usize = 0x0C; // bound, not a game offset: a byte of the save file's header
/// The bytes of a save read for its GUID.
const HEADER_READ: usize = GUID_AT + 16;
/// Where a save's header holds the Unix time it was written, a u32 (server
/// 25988414 and client 426).
const SAVED_AT: usize = 0x20; // bound, not a game offset: a byte of the save file's header
/// The backups the game keeps of a save, `backup/<stem>.ksd.backup-0` on.
const BACKUPS: u8 = 10;

/// What the loader reads of a world's save for mods.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SaveInfo {
    /// The world's GUID, the same across saves.
    pub world: Guid,
    /// When it was written, in seconds since the Unix epoch.
    pub saved_at: u32,
    /// The save file's bytes.
    pub size: u64,
    /// The backups beside it.
    pub backups: u8,
}

/// The save of the world `key`, `None` before its first save or for a file
/// without the header.
///
/// # Errors
///
/// Reading the file or its metadata failed for another reason than its
/// absence.
pub fn save_info(key: &str) -> io::Result<Option<SaveInfo>> {
    let path = PathBuf::from(format!("{key}.{SAVE_EXTENSION}"));
    let mut header = Vec::with_capacity(SAVED_AT + 4);
    match std::fs::File::open(&path) {
        Ok(file) => file.take((SAVED_AT + 4) as u64).read_to_end(&mut header)?,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    if header.len() < SAVED_AT + 4 || !header.starts_with(MAGIC) {
        return Ok(None);
    }
    let mut world = Guid::default();
    world.copy_from_slice(&header[GUID_AT..HEADER_READ]);
    let mut at = [0u8; 4];
    at.copy_from_slice(&header[SAVED_AT..SAVED_AT + 4]);
    let name = path
        .file_name()
        .map_or_default(|n| n.to_string_lossy().into_owned());
    let folder = path.parent().map_or_default(|p| p.join("backup"));
    let backups = (0..BACKUPS)
        .filter(|n| folder.join(format!("{name}.backup-{n}")).is_file())
        .count();
    Ok(Some(SaveInfo {
        world,
        saved_at: u32::from_le_bytes(at),
        size: std::fs::metadata(&path)?.len(),
        backups: u8::try_from(backups).unwrap_or(BACKUPS),
    }))
}

/// A world's GUID, in the byte order of its save.
pub type Guid = [u8; 16];

/// The settings folder of the world `key`, its save's path without `.ksd`.
#[must_use]
pub fn settings_of(key: &str) -> PathBuf {
    PathBuf::from(format!("{key}{SETTINGS_SUFFIX}"))
}

/// The data folder of the world `key`, its save's path without `.ksd`.
#[must_use]
pub fn data_of(key: &str) -> PathBuf {
    PathBuf::from(format!("{key}{DATA_SUFFIX}"))
}

/// The loader's file `name` in the data folder of the world `key`.
#[must_use]
pub fn loader_data(key: &str, name: &str) -> PathBuf {
    data_of(key)
        .join(crate::engine::settings::LOADER_ID)
        .join(name)
}

/// Writes `value` as TOML after `prefix` to `path`, atomically; a value that
/// does not serialize writes nothing, so no empty file replaces the last one.
///
/// # Errors
///
/// When `value` does not serialize or the file is not written.
pub(crate) fn write_toml(
    path: &Path,
    prefix: &str,
    value: &impl serde::Serialize,
) -> io::Result<()> {
    let body = toml::to_string(value).map_err(io::Error::other)?;
    enshwick_core::files::write_atomically(path, format!("{prefix}{body}"))
}

/// What the check found.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Identity {
    /// The folders belong to the world, or there is nothing to tell yet.
    Kept,
    /// The folders belonged to another world and now carry this suffix.
    Renamed {
        /// `.old-<date>-<time>`.
        suffix: String,
    },
    /// The save's header does not read, so the folders stay as they are.
    Unreadable,
}

/// Why [`check`] failed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CheckError {
    /// The record or the save did not read, or the record was not written;
    /// the folders are as they were, or renamed together.
    #[error(transparent)]
    Io(#[from] io::Error),
    /// A folder was not renamed, or no free name was left for it, and the
    /// folders renamed before it were renamed back, so both still belong to
    /// the other world.
    #[error("{} was not renamed ({error}), and no folder of the world was left renamed", folder.display())]
    NotRenamed {
        /// The folder that kept its name.
        folder: PathBuf,
        /// Why.
        error: io::Error,
    },
    /// A folder was not renamed, and one renamed before it was not renamed
    /// back either: the world's folders are split between two names.
    #[error("{} was not renamed ({error}), and {} was not renamed back ({undo})", folder.display(), stuck.display())]
    Split {
        /// The folder that kept its name.
        folder: PathBuf,
        /// Why.
        error: io::Error,
        /// The folder left under its new name.
        stuck: PathBuf,
        /// Why it was not renamed back.
        undo: io::Error,
    },
}

impl CheckError {
    /// Whether the folders under the world's name may belong to another
    /// world, so no mod may use them.
    #[must_use]
    pub fn foreign(&self) -> bool {
        !matches!(self, Self::Io(_))
    }
}

/// Checks that the folders of the world `key` belong to the world in its
/// save, renaming them with `stamp` (`<date>-<time>`) when they do not, and
/// records the save's GUID when none is recorded yet.
///
/// # Errors
///
/// When the record or the save cannot be read, the record not written, or a
/// folder not renamed; then the folders renamed before it are renamed back
/// ([`CheckError::foreign`]).
pub fn check(key: &str, stamp: &str) -> Result<Identity, CheckError> {
    check_with(key, stamp, &mut |from, to| std::fs::rename(from, to))
}

/// [`check`], renaming through `rename`, each rename tried again for about a
/// second while another program holds a file in the folder.
fn check_with(
    key: &str,
    stamp: &str,
    rename: &mut dyn FnMut(&Path, &Path) -> io::Result<()>,
) -> Result<Identity, CheckError> {
    let recorded = recorded(key)?;
    let save = match guid_of(&Path::new(key).with_extension(SAVE_EXTENSION)) {
        Ok(Some(guid)) => Some(guid),
        Ok(None) => return Ok(Identity::Unreadable),
        Err(e) if e.kind() == io::ErrorKind::NotFound => None,
        Err(e) => return Err(e.into()),
    };
    let identity = match (recorded, save) {
        (Some(old), Some(new)) if old == new => return Ok(Identity::Kept),
        (Some(_), _) => {
            let folders = [settings_of(key), data_of(key)];
            let suffix = free_suffix(&folders, stamp)?;
            set_aside(&folders, &suffix, &mut |from, to| {
                enshwick_base::__internal::while_busy(|| rename(from, to))
            })?;
            Identity::Renamed { suffix }
        }
        (None, _) => Identity::Kept,
    };
    if let Some(guid) = save {
        write_record(key, guid)?;
    }
    Ok(identity)
}

/// The most suffixes tried for one stamp, `.old-<stamp>` then
/// `.old-<stamp>-2` on.
const SUFFIXES: u16 = 1000;

/// The first suffix of `stamp` that no folder of `folders` carries yet.
///
/// # Errors
///
/// [`CheckError::NotRenamed`] naming the first folder when every suffix is
/// taken, as the folders then stay another world's.
fn free_suffix(folders: &[PathBuf], stamp: &str) -> Result<String, CheckError> {
    (1..=SUFFIXES)
        .map(|n| match n {
            1 => format!(".old-{stamp}"),
            n => format!(".old-{stamp}-{n}"),
        })
        .find(|suffix| {
            folders
                .iter()
                .all(|folder| !renamed(folder, suffix).exists())
        })
        .ok_or_else(|| CheckError::NotRenamed {
            folder: folders.first().cloned().unwrap_or_default(),
            error: io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("every suffix of .old-{stamp} is taken"),
            ),
        })
}

/// `folder` with `suffix` after its name.
fn renamed(folder: &Path, suffix: &str) -> PathBuf {
    let mut to = folder.as_os_str().to_owned();
    to.push(suffix);
    PathBuf::from(to)
}

/// Renames each of `folders` that exists with `suffix`; when one fails, the
/// ones renamed before it are renamed back, so the folders keep one name.
fn set_aside(
    folders: &[PathBuf],
    suffix: &str,
    rename: &mut dyn FnMut(&Path, &Path) -> io::Result<()>,
) -> Result<(), CheckError> {
    let mut moved: Vec<(&Path, PathBuf)> = Vec::with_capacity(folders.len());
    for folder in folders.iter().filter(|folder| folder.exists()) {
        let to = renamed(folder, suffix);
        let Err(error) = rename(folder, &to) else {
            moved.push((folder, to));
            continue;
        };
        let mut stuck = None;
        for (from, to) in moved.iter().rev() {
            if let Err(undo) = rename(to, from) {
                stuck.get_or_insert((to.clone(), undo));
            }
        }
        let folder = folder.clone();
        return Err(match stuck {
            None => CheckError::NotRenamed { folder, error },
            Some((stuck, undo)) => CheckError::Split {
                folder,
                error,
                stuck,
                undo,
            },
        });
    }
    Ok(())
}

/// Records the GUID of the world `key` after it saved, when none is
/// recorded: a new world has no save until its first.
///
/// # Errors
///
/// When the record or the save cannot be read, or the record not written.
pub fn note(key: &str) -> io::Result<()> {
    if recorded(key)?.is_some() {
        return Ok(());
    }
    match guid_of(&Path::new(key).with_extension(SAVE_EXTENSION))? {
        Some(guid) => write_record(key, guid),
        None => Ok(()),
    }
}

/// The GUID in the header of the save at `path`; `None` when the file is
/// shorter than the header or starts otherwise.
///
/// # Errors
///
/// When the file cannot be opened or read.
pub fn guid_of(path: &Path) -> io::Result<Option<Guid>> {
    let mut header = Vec::with_capacity(HEADER_READ);
    std::fs::File::open(path)?
        .take(HEADER_READ as u64)
        .read_to_end(&mut header)?;
    if header.len() < HEADER_READ || !header.starts_with(MAGIC) {
        return Ok(None);
    }
    let mut guid = Guid::default();
    guid.copy_from_slice(&header[GUID_AT..HEADER_READ]);
    Ok(Some(guid))
}

/// What the record holds.
#[derive(serde::Deserialize, serde::Serialize)]
struct Record {
    /// The GUID in lowercase hex, in the byte order of the save.
    guid: String,
}

fn record_path(key: &str) -> PathBuf {
    loader_data(key, RECORD_FILE)
}

/// The recorded GUID; `None` without a record, or with one that does not
/// read, which the next check replaces.
fn recorded(key: &str) -> io::Result<Option<Guid>> {
    let Some(text) = enshwick_core::files::read_to_string_if_present(&record_path(key))? else {
        return Ok(None);
    };
    Ok(toml::from_str::<Record>(&text)
        .ok()
        .and_then(|record| parse_hex(&record.guid)))
}

fn write_record(key: &str, guid: Guid) -> io::Result<()> {
    let guid = guid.iter().fold(String::with_capacity(32), |mut hex, b| {
        let _written = write!(hex, "{b:02x}");
        hex
    });
    write_toml(&record_path(key), "", &Record { guid })
}

fn parse_hex(text: &str) -> Option<Guid> {
    let mut guid = Guid::default();
    if text.len() != 32 {
        return None;
    }
    for (byte, pair) in guid.iter_mut().zip(text.as_bytes().chunks(2)) {
        *byte = u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok()?;
    }
    Some(guid)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("enshwick-folders-{name}-{}", std::process::id()));
        let _gone = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn save(key: &str, guid: u8) {
        let mut bytes = vec![0u8; 0x30];
        bytes[..4].copy_from_slice(MAGIC);
        bytes[GUID_AT..HEADER_READ].fill(guid);
        std::fs::write(Path::new(key).with_extension(SAVE_EXTENSION), bytes).unwrap();
    }

    fn put(path: &Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "a = 1\n").unwrap();
    }

    #[test]
    fn a_save_header_gives_its_world_guid() {
        let dir = scratch("guid");
        let key = dir.join("world0").to_string_lossy().into_owned();
        save(&key, 0xab);
        assert_eq!(guid_of(&dir.join("world0.ksd")).unwrap(), Some([0xab; 16]));
        std::fs::write(dir.join("short.ksd"), b"SV2C").unwrap();
        assert_eq!(guid_of(&dir.join("short.ksd")).unwrap(), None);
        std::fs::write(dir.join("other.ksd"), [1u8; 0x30]).unwrap();
        assert_eq!(guid_of(&dir.join("other.ksd")).unwrap(), None);
        assert_eq!(parse_hex(&"ab".repeat(16)), Some([0xab; 16]));
        assert_eq!(parse_hex("ab"), None);
        let _gone = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_info_reads_the_header_time_size_and_backups_and_is_none_before_a_save() {
        let dir = scratch("info");
        let key = dir.join("world0").to_string_lossy().into_owned();
        assert_eq!(save_info(&key).unwrap(), None, "no save yet");
        save(&key, 0xcd);
        let path = dir.join("world0.ksd");
        let mut bytes = std::fs::read(&path).unwrap();
        bytes[SAVED_AT..SAVED_AT + 4].copy_from_slice(&1_762_300_000u32.to_le_bytes());
        std::fs::write(&path, &bytes).unwrap();
        for n in [0, 3] {
            put(&dir.join("backup").join(format!("world0.ksd.backup-{n}")));
        }
        let info = save_info(&key).unwrap().unwrap();
        assert_eq!(info.world, [0xcd; 16]);
        assert_eq!(info.saved_at, 1_762_300_000);
        assert_eq!(info.size, 0x30);
        assert_eq!(info.backups, 2);
        std::fs::write(&path, b"SV2C").unwrap();
        assert_eq!(save_info(&key).unwrap(), None, "a header too short");
    }

    #[test]
    fn a_world_keeps_its_folders_and_a_copied_save_keeps_them_too() {
        let dir = scratch("same");
        let key = dir.join("world0").to_string_lossy().into_owned();
        save(&key, 1);
        put(&data_of(&key).join("you.lockers/owners.toml"));
        assert_eq!(check(&key, "s").unwrap(), Identity::Kept, "first use");
        assert_eq!(recorded(&key).unwrap(), Some([1; 16]));
        assert_eq!(check(&key, "s").unwrap(), Identity::Kept);
        let copy = scratch("same-copy");
        for name in [
            "world0.ksd",
            "world0-mod-data/you.lockers/owners.toml",
            "world0-mod-data/enshwick/world.toml",
        ] {
            put(&copy.join(name));
            std::fs::copy(dir.join(name), copy.join(name)).unwrap();
        }
        let copied = copy.join("world0").to_string_lossy().into_owned();
        assert_eq!(check(&copied, "s").unwrap(), Identity::Kept);
        assert!(data_of(&copied).join("you.lockers/owners.toml").exists());
        let _gone = std::fs::remove_dir_all(&dir);
        let _gone = std::fs::remove_dir_all(&copy);
    }

    #[test]
    fn a_new_world_in_a_reused_slot_starts_clean_with_the_old_folders_renamed() {
        let dir = scratch("reused");
        let key = dir.join("world2").to_string_lossy().into_owned();
        save(&key, 1);
        put(&data_of(&key).join("you.lockers/owners.toml"));
        put(&settings_of(&key).join("you.lockers/settings.toml"));
        assert_eq!(check(&key, "s").unwrap(), Identity::Kept);
        std::fs::remove_file(dir.join("world2.ksd")).unwrap();
        let renamed = Identity::Renamed {
            suffix: ".old-2026-11-02-10-00-00".to_owned(),
        };
        assert_eq!(
            check(&key, "2026-11-02-10-00-00").unwrap(),
            renamed,
            "the world was deleted"
        );
        assert!(!data_of(&key).exists() && !settings_of(&key).exists());
        assert!(
            dir.join("world2-mod-data.old-2026-11-02-10-00-00/you.lockers/owners.toml")
                .exists()
        );
        assert!(
            dir.join("world2-mod-settings.old-2026-11-02-10-00-00/you.lockers/settings.toml")
                .exists()
        );
        save(&key, 2);
        note(&key).unwrap();
        assert_eq!(
            recorded(&key).unwrap(),
            Some([2; 16]),
            "recorded after the first save"
        );
        put(&data_of(&key).join("you.lockers/owners.toml"));
        save(&key, 3);
        assert!(matches!(
            check(&key, "2026-11-02-11-00-00").unwrap(),
            Identity::Renamed { .. }
        ));
        assert_eq!(
            recorded(&key).unwrap(),
            Some([3; 16]),
            "a save of another world"
        );
        let _gone = std::fs::remove_dir_all(&dir);
    }

    /// A world in `dir` whose folders hold another world's record.
    fn foreign_world(dir: &Path) -> String {
        let key = dir.join("world3").to_string_lossy().into_owned();
        save(&key, 1);
        put(&data_of(&key).join("you.lockers/owners.toml"));
        put(&settings_of(&key).join("you.lockers/settings.toml"));
        assert_eq!(check(&key, "s").unwrap(), Identity::Kept);
        save(&key, 2);
        key
    }

    #[test]
    fn a_taken_old_name_gets_the_next_number_for_both_folders() {
        let dir = scratch("taken");
        let key = foreign_world(&dir);
        std::fs::create_dir_all(dir.join("world3-mod-data.old-s")).unwrap();
        std::fs::create_dir_all(dir.join("world3-mod-settings.old-s-2")).unwrap();
        assert_eq!(
            check(&key, "s").unwrap(),
            Identity::Renamed {
                suffix: ".old-s-3".to_owned()
            }
        );
        assert!(
            dir.join("world3-mod-data.old-s-3/you.lockers/owners.toml")
                .exists()
        );
        assert!(
            dir.join("world3-mod-settings.old-s-3/you.lockers/settings.toml")
                .exists()
        );
        let _gone = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_failed_rename_renames_the_folder_before_it_back() {
        let dir = scratch("undo");
        let key = foreign_world(&dir);
        let mut rename = |from: &Path, to: &Path| {
            if from.ends_with("world3-mod-data") {
                return Err(io::Error::other("in use"));
            }
            std::fs::rename(from, to)
        };
        let error = check_with(&key, "s", &mut rename).unwrap_err();
        assert!(
            matches!(error, CheckError::NotRenamed { .. }) && error.foreign(),
            "{error}"
        );
        assert!(settings_of(&key).join("you.lockers/settings.toml").exists());
        assert!(data_of(&key).join("you.lockers/owners.toml").exists());
        assert!(!dir.join("world3-mod-settings.old-s").exists());
        assert_eq!(
            recorded(&key).unwrap(),
            Some([1; 16]),
            "the next check sees it again"
        );
        let _gone = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_rename_back_that_fails_too_names_the_folder_left_split() {
        let dir = scratch("split");
        let key = foreign_world(&dir);
        let mut rename = |from: &Path, to: &Path| {
            if from.ends_with("world3-mod-settings") {
                return std::fs::rename(from, to);
            }
            Err(io::Error::other("in use"))
        };
        let error = check_with(&key, "s", &mut rename).unwrap_err();
        let CheckError::Split { stuck, .. } = &error else {
            panic!("{error}");
        };
        assert_eq!(stuck, &dir.join("world3-mod-settings.old-s"));
        assert!(error.foreign());
        assert!(!CheckError::Io(io::Error::other("x")).foreign());
        let _gone = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_folder_another_program_holds_for_a_moment_is_still_set_aside() {
        let dir = scratch("held");
        let key = foreign_world(&dir);
        let mut refusals = 0;
        let mut rename = |from: &Path, to: &Path| {
            if from.ends_with("world3-mod-data") && refusals < 3 {
                refusals += 1;
                return Err(io::ErrorKind::PermissionDenied.into());
            }
            std::fs::rename(from, to)
        };
        assert_eq!(
            check_with(&key, "s", &mut rename).unwrap(),
            Identity::Renamed {
                suffix: ".old-s".to_owned()
            }
        );
        assert!(
            dir.join("world3-mod-data.old-s/you.lockers/owners.toml")
                .exists()
        );
        let _gone = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn folders_with_every_old_name_taken_stay_another_world_s() {
        let dir = scratch("every-name");
        let key = foreign_world(&dir);
        for n in 1..=SUFFIXES {
            let suffix = if n == 1 {
                ".old-s".to_owned()
            } else {
                format!(".old-s-{n}")
            };
            std::fs::create_dir(renamed(&data_of(&key), &suffix)).unwrap();
        }
        let error = check(&key, "s").unwrap_err();
        assert!(
            matches!(error, CheckError::NotRenamed { .. }) && error.foreign(),
            "{error}"
        );
        assert!(data_of(&key).join("you.lockers/owners.toml").exists());
        let _gone = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_value_that_does_not_serialize_leaves_the_last_file_as_it_was() {
        let dir = scratch("toml");
        let path = dir.join("held.toml");
        write_toml(&path, "# kept\n", &Record { guid: "ab".into() }).unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "# kept\nguid = \"ab\"\n"
        );
        assert!(write_toml(&path, "# new\n", &7u8).is_err(), "no table");
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "# kept\nguid = \"ab\"\n"
        );
        let _gone = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_save_without_a_readable_header_keeps_the_folders() {
        let dir = scratch("unreadable");
        let key = dir.join("world0").to_string_lossy().into_owned();
        save(&key, 1);
        put(&data_of(&key).join("you.lockers/owners.toml"));
        check(&key, "s").unwrap();
        std::fs::write(dir.join("world0.ksd"), b"broken").unwrap();
        assert_eq!(check(&key, "s").unwrap(), Identity::Unreadable);
        assert!(data_of(&key).join("you.lockers/owners.toml").exists());
        let _gone = std::fs::remove_dir_all(&dir);
    }
}
