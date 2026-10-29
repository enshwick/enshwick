//! A world whose save moved to another folder keeps its mods' data.
//!
//! A world key holds a hash of its save's folder, so moving a server
//! install, a Steam library, or a hosted world to a dedicated server gives
//! the world a new key. When a world that has a save but no data under its
//! key appears, and exactly one other key of the same save name has data,
//! the loader takes that data over: it moves the folders when the old save
//! is gone, and copies them when the old save is still there, since the
//! world was copied. The loader keeps each world's save path in its own
//! folder for world `key`, as [`SAVE_NOTE`], to tell the two apart.

use std::io;
use std::path::{Path, PathBuf};

use enshwick::state::WORLDS_DIR;

/// The file in the loader's folder for a world that names the world's save.
pub const SAVE_NOTE: &str = "save.toml";

/// What became of a world's data as it appeared.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Arrival {
    /// The world has data under its key already, or has none to take over.
    Settled,
    /// The data of the world `from`, whose save is gone, moved to this key.
    Moved {
        /// The old key.
        from: String,
    },
    /// The data of the world `from`, whose save is still there, was copied
    /// to this key.
    Copied {
        /// The old key.
        from: String,
    },
    /// Several old keys of the same save name have data, so none is taken;
    /// an admin moves the right folders by hand.
    Unclear(Vec<String>),
}

/// What the loader keeps about a world's save.
#[derive(Debug, Default, serde::Deserialize, serde::Serialize)]
#[serde(default)]
struct SaveNote {
    save: String,
}

/// The world with `key` and the save file `save` appears: takes over the
/// data of the world it moved or was copied from, see [the module](self),
/// and notes its save in the loader's folder for it.
///
/// # Errors
///
/// The folder that could not be read, moved or copied; whatever moved is
/// moved back, and whatever was copied removed.
pub fn arrive(data_dir: &Path, loader_id: &str, key: &str, save: &Path) -> io::Result<Arrival> {
    let arrival = take_over(data_dir, loader_id, key, save)?;
    let note = data_dir
        .join(loader_id)
        .join(WORLDS_DIR)
        .join(key)
        .join(SAVE_NOTE);
    let saved = save.to_string_lossy().into_owned();
    if read_note(&note).as_deref() != Some(saved.as_str()) && save.is_file() {
        let text = toml::to_string(&SaveNote { save: saved }).map_err(io::Error::other)?;
        enshwick::files::write_atomically(&note, text)?;
    }
    Ok(arrival)
}

/// The other world keys of `key`'s save name that hold mod data, sorted:
/// the worlds a world may have moved or been copied from.
///
/// # Errors
///
/// When a mod's folder of worlds does not read.
pub fn others(data_dir: &Path, key: &str) -> io::Result<Vec<String>> {
    let Some(stem) = stem_of(key) else {
        return Ok(Vec::new());
    };
    let mut others: Vec<String> = world_folders(data_dir)?
        .into_iter()
        .map(|w| w.key)
        .filter(|other| other != key && stem_of(other) == Some(stem))
        .collect();
    others.sort_unstable();
    others.dedup();
    Ok(others)
}

fn take_over(data_dir: &Path, loader_id: &str, key: &str, save: &Path) -> io::Result<Arrival> {
    if stem_of(key).is_none() || !save.is_file() {
        return Ok(Arrival::Settled);
    }
    let worlds = world_folders(data_dir)?;
    if worlds.iter().any(|w| w.key == key) {
        return Ok(Arrival::Settled);
    }
    let others = others(data_dir, key)?;
    let from = match others.as_slice() {
        [] => return Ok(Arrival::Settled),
        [from] => from.clone(),
        _ => {
            return Ok(Arrival::Unclear(others));
        }
    };
    let old_note = data_dir
        .join(loader_id)
        .join(WORLDS_DIR)
        .join(&from)
        .join(SAVE_NOTE);
    let old_save_there = read_note(&old_note).is_some_and(|old| Path::new(&old).is_file());
    let moving: Vec<(PathBuf, PathBuf)> = worlds
        .iter()
        .filter(|w| w.key == from)
        .map(|w| (w.path.clone(), w.path.with_file_name(key)))
        .collect();
    if old_save_there {
        copy_all(&moving)?;
        Ok(Arrival::Copied { from })
    } else {
        move_all(&moving)?;
        Ok(Arrival::Moved { from })
    }
}

/// Moves each folder to its new name; on a failure moves the moved ones
/// back.
fn move_all(moving: &[(PathBuf, PathBuf)]) -> io::Result<()> {
    for (done, (from, to)) in moving.iter().enumerate() {
        if let Err(e) = std::fs::rename(from, to) {
            for (back_from, back_to) in moving.iter().take(done) {
                let _back = std::fs::rename(back_to, back_from);
            }
            return Err(io::Error::new(e.kind(), format!("{}: {e}", from.display())));
        }
    }
    Ok(())
}

/// Copies each folder to its new name; on a failure removes the copies.
fn copy_all(copying: &[(PathBuf, PathBuf)]) -> io::Result<()> {
    for (done, (from, to)) in copying.iter().enumerate() {
        if let Err(e) = copy_folder(from, to) {
            for (_, copy) in copying.iter().take(done + 1) {
                let _gone = std::fs::remove_dir_all(copy);
            }
            return Err(io::Error::new(e.kind(), format!("{}: {e}", from.display())));
        }
    }
    Ok(())
}

fn copy_folder(from: &Path, to: &Path) -> io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_folder(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

/// The save path a world's note names; `None` without a note that reads.
fn read_note(path: &Path) -> Option<String> {
    let text = enshwick::files::read_to_string_if_present(path).ok()??;
    toml::from_str::<SaveNote>(&text).ok().map(|note| note.save)
}

/// The save name of a world key: the key without its folder hash.
fn stem_of(key: &str) -> Option<&str> {
    let (stem, hash) = key.rsplit_once('-')?;
    (!stem.is_empty() && hash.len() == 8 && hash.bytes().all(|b| b.is_ascii_hexdigit()))
        .then_some(stem)
}

/// A world's folder under one mod's data folder.
struct WorldFolder {
    key: String,
    path: PathBuf,
}

/// Every world folder under every mod's data folder.
fn world_folders(data_dir: &Path) -> io::Result<Vec<WorldFolder>> {
    let mut found = Vec::new();
    let mods = match std::fs::read_dir(data_dir) {
        Ok(mods) => mods,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(found),
        Err(e) => return Err(e),
    };
    for entry in mods {
        let worlds = entry?.path().join(WORLDS_DIR);
        let entries = match std::fs::read_dir(&worlds) {
            Ok(entries) => entries,
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::NotFound | io::ErrorKind::NotADirectory
                ) =>
            {
                continue;
            }
            Err(e) => return Err(e),
        };
        for world in entries {
            let world = world?;
            if !world.file_type()?.is_dir() {
                continue;
            }
            if let Some(key) = world.file_name().to_str() {
                found.push(WorldFolder {
                    key: key.to_owned(),
                    path: world.path(),
                });
            }
        }
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOADER: &str = "enshwick";

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("enshwick-moved-{name}-{}", std::process::id()));
        let _gone = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn put(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    /// A data folder where `world0-aaaaaaaa` has a lock store and settings,
    /// with its save at `old/world0.ksd` under `root`.
    fn installed(name: &str) -> (PathBuf, PathBuf) {
        let root = scratch(name);
        let data = root.join("mod-data");
        put(
            &data.join("you.lockers/worlds/world0-aaaaaaaa/owners.toml"),
            "a = 1\n",
        );
        put(
            &data.join("you.lockers/worlds/world0-aaaaaaaa/settings.toml"),
            "limit = 2\n",
        );
        put(
            &data.join("enshwick/worlds/world0-aaaaaaaa/attachments.bin"),
            "x",
        );
        put(&data.join("you.lockers/worlds/next.toml"), "");
        let old = root.join("old/world0.ksd");
        put(&old, "save");
        let note = SaveNote {
            save: old.to_string_lossy().into_owned(),
        };
        put(
            &data.join("enshwick/worlds/world0-aaaaaaaa").join(SAVE_NOTE),
            &toml::to_string(&note).unwrap(),
        );
        (root, data)
    }

    #[test]
    fn a_moved_world_takes_its_data_along() {
        let (root, data) = installed("move");
        std::fs::remove_file(root.join("old/world0.ksd")).unwrap();
        let new = root.join("new/world0.ksd");
        put(&new, "save");
        assert_eq!(
            arrive(&data, LOADER, "world0-bbbbbbbb", &new).unwrap(),
            Arrival::Moved {
                from: "world0-aaaaaaaa".to_owned()
            }
        );
        let moved = data.join("you.lockers/worlds/world0-bbbbbbbb");
        assert_eq!(
            std::fs::read_to_string(moved.join("owners.toml")).unwrap(),
            "a = 1\n"
        );
        assert!(
            data.join("enshwick/worlds/world0-bbbbbbbb/attachments.bin")
                .exists()
        );
        assert!(!data.join("you.lockers/worlds/world0-aaaaaaaa").exists());
        assert_eq!(
            read_note(&data.join("enshwick/worlds/world0-bbbbbbbb").join(SAVE_NOTE)),
            Some(new.to_string_lossy().into_owned())
        );
        assert_eq!(
            arrive(&data, LOADER, "world0-bbbbbbbb", &new).unwrap(),
            Arrival::Settled
        );
        let _gone = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_copied_world_gets_a_copy_and_the_old_keeps_its_data() {
        let (root, data) = installed("copy");
        let new = root.join("new/world0.ksd");
        put(&new, "save");
        assert_eq!(
            arrive(&data, LOADER, "world0-bbbbbbbb", &new).unwrap(),
            Arrival::Copied {
                from: "world0-aaaaaaaa".to_owned()
            }
        );
        for key in ["world0-aaaaaaaa", "world0-bbbbbbbb"] {
            let world = data.join("you.lockers/worlds").join(key);
            assert_eq!(
                std::fs::read_to_string(world.join("owners.toml")).unwrap(),
                "a = 1\n"
            );
        }
        let _gone = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_data_folder_from_before_save_notes_moves() {
        let (root, data) = installed("no-note");
        std::fs::remove_file(data.join("enshwick/worlds/world0-aaaaaaaa").join(SAVE_NOTE)).unwrap();
        let new = root.join("new/world0.ksd");
        put(&new, "save");
        assert!(matches!(
            arrive(&data, LOADER, "world0-bbbbbbbb", &new).unwrap(),
            Arrival::Moved { .. }
        ));
        let _gone = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_new_world_or_one_of_another_name_takes_nothing() {
        let (root, data) = installed("new");
        let fresh = root.join("new/world0.ksd");
        assert_eq!(
            arrive(&data, LOADER, "world0-bbbbbbbb", &fresh).unwrap(),
            Arrival::Settled,
            "no save yet: a new world"
        );
        let other = root.join("new/world1.ksd");
        put(&other, "save");
        assert_eq!(
            arrive(&data, LOADER, "world1-bbbbbbbb", &other).unwrap(),
            Arrival::Settled
        );
        assert!(
            data.join("you.lockers/worlds/world0-aaaaaaaa/owners.toml")
                .exists()
        );
        let _gone = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn two_candidates_are_unclear_and_nothing_moves() {
        let (root, data) = installed("unclear");
        put(
            &data.join("you.lockers/worlds/world0-cccccccc/owners.toml"),
            "a = 3\n",
        );
        let new = root.join("new/world0.ksd");
        put(&new, "save");
        assert_eq!(
            arrive(&data, LOADER, "world0-bbbbbbbb", &new).unwrap(),
            Arrival::Unclear(vec![
                "world0-aaaaaaaa".to_owned(),
                "world0-cccccccc".to_owned()
            ])
        );
        assert!(!data.join("you.lockers/worlds/world0-bbbbbbbb").exists());
        let _gone = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_other_worlds_of_a_save_name_are_listed_once() {
        let (root, data) = installed("others");
        put(
            &data.join("you.lockers/worlds/world0-cccccccc/owners.toml"),
            "a = 3\n",
        );
        put(
            &data.join("you.lockers/worlds/world1-dddddddd/owners.toml"),
            "a = 4\n",
        );
        assert_eq!(
            others(&data, "world0-bbbbbbbb").unwrap(),
            ["world0-aaaaaaaa", "world0-cccccccc"]
        );
        assert_eq!(
            others(&data, "world0-aaaaaaaa").unwrap(),
            ["world0-cccccccc"]
        );
        let _gone = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_save_name_is_the_key_without_its_hash() {
        assert_eq!(stem_of("world0-6017246f"), Some("world0"));
        assert_eq!(stem_of("my-world-6017246f"), Some("my-world"));
        assert_eq!(stem_of("world0-xyz"), None);
        assert_eq!(stem_of("-6017246f"), None);
    }
}
