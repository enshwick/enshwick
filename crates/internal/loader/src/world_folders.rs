//! The two folders beside a world's save, and the check that they belong to
//! the world in that save.
//!
//! A world's mod settings live in `<stem>-mod-settings/` and the data mods
//! write for it in `<stem>-mod-data/`, beside `<stem>.ksd`. The stem is a
//! slot the game gives the next new world once a world is deleted, and the
//! game leaves the folders behind (client 26320373). So the loader records
//! the world's GUID in `<stem>-mod-data/enshwick/world.toml` and, when a
//! save in the slot holds another GUID or is gone, renames both folders to
//! `<name>.old-<date>-<time>`, so the new world starts from the mods'
//! defaults and nothing is deleted.
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
    data_of(key).join(crate::settings::LOADER_ID).join(name)
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

/// Checks that the folders of the world `key` belong to the world in its
/// save, renaming them with `stamp` (`<date>-<time>`) when they do not, and
/// records the save's GUID when none is recorded yet.
///
/// # Errors
///
/// When the record or the save cannot be read, or a folder not renamed or
/// the record not written; what was renamed stays renamed.
pub fn check(key: &str, stamp: &str) -> io::Result<Identity> {
    let recorded = recorded(key)?;
    let save = match guid_of(&Path::new(key).with_extension(SAVE_EXTENSION)) {
        Ok(Some(guid)) => Some(guid),
        Ok(None) => return Ok(Identity::Unreadable),
        Err(e) if e.kind() == io::ErrorKind::NotFound => None,
        Err(e) => return Err(e),
    };
    let identity = match (recorded, save) {
        (Some(old), Some(new)) if old == new => return Ok(Identity::Kept),
        (Some(_), _) => {
            let suffix = format!(".old-{stamp}");
            for folder in [settings_of(key), data_of(key)] {
                if folder.exists() {
                    let mut to = folder.into_os_string();
                    let from = PathBuf::from(&to);
                    to.push(&suffix);
                    std::fs::rename(&from, &to)?;
                }
            }
            Identity::Renamed { suffix }
        }
        (None, _) => Identity::Kept,
    };
    if let Some(guid) = save {
        write_record(key, guid)?;
    }
    Ok(identity)
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
    let Some(text) = enshwick::files::read_to_string_if_present(&record_path(key))? else {
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
    let text = toml::to_string(&Record { guid }).map_err(io::Error::other)?;
    enshwick::files::write_atomically(&record_path(key), text)
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
