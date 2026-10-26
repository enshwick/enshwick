//! A renamed mod's settings and data, carried from an id it had to its id.
//!
//! On the first start that runs a mod with a former id (`was` in its
//! manifest), the loader copies `mod-configs/<old>.toml`, its texts file and
//! everything under `mod-data/<old>/` (each world's settings and stores, a
//! hold) to the new id, and the migrations that ran under the old id count
//! as run under the new one. A file the new id has already stays. The old
//! files stay too, for a player who goes back to the old version. The record
//! in `mod-data/enshwick/migrations.toml` keeps what was carried, so it is
//! carried once.

use std::path::{Path, PathBuf};

use enshwick::abi::Level;

use crate::migrations::{RECORD, Record};

/// Where mods keep their settings and data.
#[derive(Clone, Copy, Debug)]
pub struct Dirs<'a> {
    /// The `mod-configs` folder.
    pub configs: &'a Path,
    /// The `mod-data` folder.
    pub data: &'a Path,
}

/// Carries the settings and data of the former id `old` to the mod `id`, unless
/// `record` says it was carried; returns how many files were copied, or `None`
/// when it was carried before.
///
/// In a dry run nothing is copied or recorded, and the count is what would be
/// copied.
///
/// # Errors
///
/// Why a file could not be read or written; what was copied stays, nothing
/// is recorded, and the next start copies the rest.
pub fn carry(
    dirs: Dirs<'_>,
    record: &mut Record,
    id: &str,
    old: &str,
    dry: bool,
) -> Result<Option<usize>, String> {
    if record
        .carried
        .get(id)
        .is_some_and(|ids| ids.iter().any(|o| o == old))
    {
        return Ok(None);
    }
    let mut copies: Vec<(PathBuf, PathBuf)> = Vec::new();
    for suffix in [".toml", ".texts.toml"] {
        copies.push((
            dirs.configs.join(format!("{old}{suffix}")),
            dirs.configs.join(format!("{id}{suffix}")),
        ));
    }
    let old_data = dirs.data.join(old);
    let new_data = dirs.data.join(id);
    files(&old_data, &mut |file| {
        if let Ok(rest) = file.strip_prefix(&old_data) {
            copies.push((file.to_path_buf(), new_data.join(rest)));
        }
    })?;
    copies.retain(|(from, to)| from.is_file() && !to.exists());
    if dry {
        return Ok(Some(copies.len()));
    }
    for (from, to) in &copies {
        if let Some(dir) = to.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        std::fs::copy(from, to).map_err(|e| format!("{}: {e}", from.display()))?;
    }
    record.take_over(id, old);
    Ok(Some(copies.len()))
}

/// A line for the log under a mod's id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    /// The mod the line is about.
    pub id: String,
    /// How much it matters.
    pub level: Level,
    /// What happened.
    pub text: String,
}

/// Carries the former ids of every mod in `mods`, each a mod id with the ids
/// it had, and keeps the record in `dirs.data` up to date; returns what to
/// log.
#[must_use]
pub fn carry_all(dirs: Dirs<'_>, mods: &[(&str, Vec<&str>)], dry: bool) -> Vec<Line> {
    let mut lines = Vec::new();
    if mods.iter().all(|(_, former)| former.is_empty()) {
        return lines;
    }
    let mut record = match Record::read(dirs.data) {
        Ok(record) => record,
        Err(e) => {
            for (id, _) in mods.iter().filter(|(_, former)| !former.is_empty()) {
                lines.push(Line {
                    id: (*id).to_owned(),
                    level: Level::WARN,
                    text: format!(
                        "settings of former ids not carried over: mod-data/{RECORD} does not read ({e})"
                    ),
                });
            }
            return lines;
        }
    };
    let mut changed = false;
    for (id, former) in mods {
        for old in former {
            let (level, text) = match carry(dirs, &mut record, id, old, dry) {
                Ok(None) => continue,
                Ok(Some(count)) if dry => (
                    Level::INFO,
                    format!(
                        "dry run, nothing copied: {} of {old} would carry over",
                        files_count(count)
                    ),
                ),
                Ok(Some(count)) => {
                    changed = true;
                    (
                        Level::INFO,
                        format!(
                            "carried {} of settings and data over from {old}",
                            files_count(count)
                        ),
                    )
                }
                Err(e) => (
                    Level::WARN,
                    format!("settings and data of {old} not carried over: {e}"),
                ),
            };
            lines.push(Line {
                id: (*id).to_owned(),
                level,
                text,
            });
        }
    }
    if changed && let Err(e) = record.write(dirs.data) {
        lines.push(Line {
            id: mods[0].0.to_owned(),
            level: Level::WARN,
            text: format!("mod-data/{RECORD} not written: {e}"),
        });
    }
    lines
}

/// `1 file`, `2 files`.
fn files_count(count: usize) -> String {
    if count == 1 {
        "1 file".to_owned()
    } else {
        format!("{count} files")
    }
}

/// Calls `found` with every file under `dir`; nothing when `dir` is missing.
fn files(dir: &Path, found: &mut impl FnMut(&Path)) -> Result<(), String> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(format!("{}: {e}", dir.display())),
    };
    for entry in entries {
        let path = entry.map_err(|e| format!("{}: {e}", dir.display()))?.path();
        if path.is_dir() {
            files(&path, found)?;
        } else {
            found(&path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("enshwick-carry-{name}-{}", std::process::id()));
        let _gone = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("mod-configs")).unwrap();
        std::fs::create_dir_all(dir.join("mod-data")).unwrap();
        dir
    }

    fn put(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn read(path: &Path) -> String {
        std::fs::read_to_string(path).unwrap()
    }

    #[test]
    fn a_renamed_mod_takes_its_settings_worlds_and_hold_once() {
        let root = scratch("once");
        let (configs, data) = (root.join("mod-configs"), root.join("mod-data"));
        let dirs = Dirs {
            configs: &configs,
            data: &data,
        };
        put(&configs.join("altars.toml"), "limit = 4\n");
        put(&data.join("altars/worlds/w-1/settings.toml"), "limit = 6\n");
        put(&data.join("altars/worlds/w-1/owners.toml"), "a = 1\n");
        put(&data.join("altars/hold.txt"), "a world left behind");
        let mut record = Record::default();
        record.ran(None, "altars").push("2".into());
        record.ran(Some("w-1"), "altars").push("2".into());

        let copied = carry(dirs, &mut record, "me.altars", "altars", false).unwrap();
        assert_eq!(copied, Some(4));
        assert_eq!(read(&configs.join("me.altars.toml")), "limit = 4\n");
        let world = data.join("me.altars/worlds/w-1");
        assert_eq!(read(&world.join("settings.toml")), "limit = 6\n");
        assert_eq!(read(&world.join("owners.toml")), "a = 1\n");
        assert_eq!(
            read(&data.join("me.altars/hold.txt")),
            "a world left behind"
        );
        assert!(configs.join("altars.toml").exists(), "the old files stay");
        assert_eq!(record.ran(None, "me.altars"), &["2"]);
        assert_eq!(record.ran(Some("w-1"), "me.altars"), &["2"]);

        put(&configs.join("me.altars.toml"), "limit = 9\n");
        std::fs::remove_file(world.join("owners.toml")).unwrap();
        assert_eq!(
            carry(dirs, &mut record, "me.altars", "altars", false),
            Ok(None)
        );
        assert_eq!(read(&configs.join("me.altars.toml")), "limit = 9\n");
        assert!(!world.join("owners.toml").exists(), "carried once");
        let _gone = std::fs::remove_dir_all(root);
    }

    #[test]
    fn files_the_new_id_has_stay_and_a_dry_run_only_counts() {
        let root = scratch("keep");
        let (configs, data) = (root.join("mod-configs"), root.join("mod-data"));
        let dirs = Dirs {
            configs: &configs,
            data: &data,
        };
        put(&configs.join("old.toml"), "limit = 4\n");
        put(&configs.join("new.toml"), "limit = 8\n");
        put(&data.join("old/worlds/w-1/settings.toml"), "limit = 6\n");
        let mut record = Record::default();

        assert_eq!(carry(dirs, &mut record, "new", "old", true), Ok(Some(1)));
        assert!(!data.join("new").exists());
        assert!(record.carried.is_empty());

        assert_eq!(carry(dirs, &mut record, "new", "old", false), Ok(Some(1)));
        assert_eq!(read(&configs.join("new.toml")), "limit = 8\n");
        assert_eq!(
            read(&data.join("new/worlds/w-1/settings.toml")),
            "limit = 6\n"
        );
        assert_eq!(record.carried["new"], ["old"]);
        let _gone = std::fs::remove_dir_all(root);
    }

    #[test]
    fn every_former_id_is_carried_at_start_and_the_record_kept() {
        let root = scratch("all");
        let (configs, data) = (root.join("mod-configs"), root.join("mod-data"));
        let dirs = Dirs {
            configs: &configs,
            data: &data,
        };
        put(&configs.join("a.toml"), "x = 1\n");
        let mods = [("me.a", vec!["a", "older-a"]), ("me.b", vec![])];
        let lines = carry_all(dirs, &mods, false);
        assert_eq!(
            lines,
            [
                Line {
                    id: "me.a".into(),
                    level: Level::INFO,
                    text: "carried 1 file of settings and data over from a".into(),
                },
                Line {
                    id: "me.a".into(),
                    level: Level::INFO,
                    text: "carried 0 files of settings and data over from older-a".into(),
                },
            ]
        );
        assert_eq!(
            Record::read(&data).unwrap().carried["me.a"],
            ["a", "older-a"]
        );
        assert!(carry_all(dirs, &mods, false).is_empty());
        let _gone = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_former_id_with_nothing_left_is_recorded_as_carried() {
        let root = scratch("empty");
        let (configs, data) = (root.join("mod-configs"), root.join("mod-data"));
        let dirs = Dirs {
            configs: &configs,
            data: &data,
        };
        let mut record = Record::default();
        assert_eq!(carry(dirs, &mut record, "new", "old", false), Ok(Some(0)));
        assert_eq!(carry(dirs, &mut record, "new", "old", false), Ok(None));
        let _gone = std::fs::remove_dir_all(root);
    }
}
