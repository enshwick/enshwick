//! Staged updates: files an updater left in `mods/.staged/`, moved into place
//! before the loader starts.
//!
//! `mods/.staged/<path>` replaces `<game folder>/<path>`, and the file it
//! replaces stays beside it as `<path>.previous` for one rollback. A file
//! that cannot be moved stays staged for the next start; nothing here stops
//! the game from starting.

use std::fmt::Write as _;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

/// The staging folder, relative to the game's folder.
const STAGED: &str = "mods/.staged";
/// The stub's log, relative to the game's folder.
const LOG: &str = "logs/enshwick/stub.log";
/// How deep the staging folder is walked.
const MAX_DEPTH: usize = 8;

/// What became of one staged file.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Outcome {
    /// The file's path relative to the game's folder.
    pub(crate) path: PathBuf,
    /// `Ok(true)` when it replaced a file, kept as `.previous`; `Ok(false)`
    /// when it was new; why it stays staged otherwise.
    pub(crate) result: Result<bool, String>,
}

/// Moves every staged file under `game` into place and logs what it did; a
/// no-op without staged files.
pub(crate) fn apply_and_log(game: &Path) {
    let outcomes = apply(game);
    if !outcomes.is_empty() {
        log(game, &outcomes);
    }
}

/// Moves every staged file under `game` into place.
pub(crate) fn apply(game: &Path) -> Vec<Outcome> {
    let staged = game.join(STAGED);
    let mut files = Vec::new();
    collect(&staged, Path::new(""), 0, &mut files);
    files.sort();
    let outcomes = files
        .into_iter()
        .map(|relative| {
            let result = install(&staged.join(&relative), &game.join(&relative));
            Outcome {
                path: relative,
                result,
            }
        })
        .collect();
    prune(&staged);
    outcomes
}

/// Adds the regular files under `dir` to `out`, as paths relative to the
/// staging folder; links are skipped.
fn collect(dir: &Path, relative: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth > MAX_DEPTH {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let path = relative.join(entry.file_name());
        if kind.is_dir() {
            collect(&entry.path(), &path, depth + 1, out);
        } else if kind.is_file() {
            out.push(path);
        }
    }
}

/// Moves `staged` to `target`, keeping a file already there as
/// `<target>.previous`.
fn install(staged: &Path, target: &Path) -> Result<bool, String> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("its folder: {e}"))?;
    }
    if target.is_dir() {
        return Err("a folder of that name is in the way".into());
    }
    let replaces = target.exists();
    let previous = previous_of(target);
    if replaces {
        if previous.exists() {
            fs::remove_file(&previous).map_err(|e| format!("the old .previous: {e}"))?;
        }
        fs::rename(target, &previous).map_err(|e| format!("keeping the current file: {e}"))?;
    }
    if let Err(error) = fs::rename(staged, target) {
        if replaces {
            // Put the current file back; the update stays staged.
            let _restored = fs::rename(&previous, target);
        }
        return Err(format!("moving it: {error}"));
    }
    Ok(replaces)
}

/// `<file>.previous` beside `file`.
fn previous_of(file: &Path) -> PathBuf {
    let mut name = file.as_os_str().to_owned();
    name.push(".previous");
    PathBuf::from(name)
}

/// Removes the empty folders under `dir`, keeping `dir` itself.
fn prune(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        if entry.file_type().is_ok_and(|k| k.is_dir()) {
            prune(&entry.path());
            // Fails, as it should, while anything stays inside.
            let _removed = fs::remove_dir(entry.path());
        }
    }
}

/// Appends one line per outcome to the stub's log.
fn log(game: &Path, outcomes: &[Outcome]) {
    let path = game.join(LOG);
    if let Some(parent) = path.parent()
        && fs::create_dir_all(parent).is_err()
    {
        return;
    }
    let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(&path) else {
        return;
    };
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let mut text = String::new();
    for outcome in outcomes {
        let path = outcome.path.to_string_lossy().replace('\\', "/");
        let what = match &outcome.result {
            Ok(true) => format!("updated {path}, the old file kept as {path}.previous"),
            Ok(false) => format!("installed {path}"),
            Err(reason) => format!("{path} stays staged: {reason}"),
        };
        let _formatted = writeln!(text, "{seconds} {what}");
    }
    let _written = file.write_all(text.as_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh, empty game folder for one test.
    fn game(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "enshwick-loader-stub-{name}-{}",
            std::process::id()
        ));
        let _cleared = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn stage(game: &Path, relative: &str, text: &str) {
        let path = game.join(STAGED).join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn read(game: &Path, relative: &str) -> String {
        fs::read_to_string(game.join(relative)).unwrap()
    }

    #[test]
    fn nothing_staged_does_nothing() {
        let game = game("nothing");
        assert!(apply(&game).is_empty());
        apply_and_log(&game);
        assert!(!game.join(LOG).exists());
    }

    #[test]
    fn a_staged_file_replaces_the_current_one() {
        let game = game("replace");
        fs::create_dir_all(game.join("mods")).unwrap();
        fs::write(game.join("mods/safes.dll"), "old").unwrap();
        fs::write(game.join("mods/safes.dll.previous"), "older").unwrap();
        stage(&game, "mods/safes.dll", "new");
        let outcomes = apply(&game);
        assert_eq!(
            outcomes,
            [Outcome {
                path: PathBuf::from("mods").join("safes.dll"),
                result: Ok(true),
            }]
        );
        assert_eq!(read(&game, "mods/safes.dll"), "new");
        assert_eq!(read(&game, "mods/safes.dll.previous"), "old");
        assert!(!game.join(STAGED).join("mods").exists());
        assert!(game.join(STAGED).exists());
    }

    #[test]
    fn new_files_go_into_new_folders() {
        let game = game("new");
        stage(&game, "mods/enshwick/enshwick.dll", "loader");
        stage(&game, "powrprof.dll", "stub");
        let outcomes = apply(&game);
        assert_eq!(outcomes.len(), 2);
        assert!(outcomes.iter().all(|o| o.result == Ok(false)));
        assert_eq!(read(&game, "mods/enshwick/enshwick.dll"), "loader");
        assert_eq!(read(&game, "powrprof.dll"), "stub");
        assert!(!game.join("powrprof.dll.previous").exists());
    }

    #[test]
    fn a_file_that_cannot_move_stays_staged() {
        let game = game("blocked");
        fs::create_dir_all(game.join("mods/safes.dll")).unwrap();
        stage(&game, "mods/safes.dll", "new");
        stage(&game, "mods/zones.dll", "zones");
        let outcomes = apply(&game);
        assert!(outcomes[0].result.is_err(), "{outcomes:?}");
        assert_eq!(outcomes[1].result, Ok(false));
        assert_eq!(read(&game, "mods/.staged/mods/safes.dll"), "new");
        assert_eq!(read(&game, "mods/zones.dll"), "zones");
        apply_and_log(&game);
        let log = read(&game, LOG);
        assert!(log.contains("mods/safes.dll stays staged"), "{log}");
    }
}
