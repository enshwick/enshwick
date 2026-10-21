//! Files `pin` writes, held back until every one is ready, then written all
//! or none: each goes to a temporary file beside its place first, and only
//! when all of them are there do they move into place. A move that fails
//! puts back what the earlier ones replaced.

use std::fs;
use std::path::{Path, PathBuf};

/// One file to write: its new text, and what it held before, if it existed.
#[derive(Debug)]
struct File {
    path: PathBuf,
    text: String,
    before: Option<String>,
}

/// The files of one run, written by [`Staged::commit`].
#[derive(Debug, Default)]
pub(crate) struct Staged(Vec<File>);

impl Staged {
    /// Adds a file that must not exist yet.
    pub(crate) fn create(&mut self, path: PathBuf, text: String) {
        self.0.push(File {
            path,
            text,
            before: None,
        });
    }

    /// Adds a new text for the file at `path`, which held `before`.
    pub(crate) fn replace(&mut self, path: PathBuf, text: String, before: String) {
        self.0.push(File {
            path,
            text,
            before: Some(before),
        });
    }

    /// Writes every file, or none of them.
    pub(crate) fn commit(self) -> Result<(), String> {
        let mut temps = Vec::with_capacity(self.0.len());
        for file in &self.0 {
            let temp = temp_of(&file.path);
            if let Err(e) = fs::write(&temp, &file.text) {
                remove_all(&temps);
                let _gone = fs::remove_file(&temp);
                return Err(format!("{}: {e}", temp.display()));
            }
            temps.push(temp);
        }
        for (done, (file, temp)) in self.0.iter().zip(&temps).enumerate() {
            if file.before.is_none() && file.path.exists() {
                return Err(self.undo(
                    done,
                    &temps,
                    &format!("{} already exists", file.path.display()),
                ));
            }
            if let Err(e) = fs::rename(temp, &file.path) {
                return Err(self.undo(done, &temps, &format!("{}: {e}", file.path.display())));
            }
        }
        Ok(())
    }

    /// Puts back the first `done` files, removes the temporary files left,
    /// and returns `why` with what could not be put back.
    fn undo(&self, done: usize, temps: &[PathBuf], why: &str) -> String {
        remove_all(&temps[done..]);
        let mut stuck = Vec::new();
        for file in &self.0[..done] {
            let back = match &file.before {
                Some(text) => fs::write(&file.path, text),
                None => fs::remove_file(&file.path),
            };
            if back.is_err() {
                stuck.push(file.path.display().to_string());
            }
        }
        if stuck.is_empty() {
            format!("{why}; nothing was written")
        } else {
            format!("{why}; could not put back {}", stuck.join(", "))
        }
    }
}

/// The temporary file beside `path`.
fn temp_of(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_owned();
    name.push(format!(".pin-{}", std::process::id()));
    path.with_file_name(name)
}

fn remove_all(paths: &[PathBuf]) {
    for path in paths {
        let _gone = fs::remove_file(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("enshwick-pin-{name}-{}", std::process::id()));
        let _old = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn every_file_lands() {
        let dir = scratch("lands");
        fs::write(dir.join("old.rs"), "old").unwrap();
        let mut staged = Staged::default();
        staged.create(dir.join("new.toml"), "table".into());
        staged.replace(dir.join("old.rs"), "ported".into(), "old".into());
        staged.commit().unwrap();
        assert_eq!(fs::read_to_string(dir.join("new.toml")).unwrap(), "table");
        assert_eq!(fs::read_to_string(dir.join("old.rs")).unwrap(), "ported");
        assert_eq!(
            fs::read_dir(&dir).unwrap().count(),
            2,
            "no temporary file left"
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_failed_file_leaves_every_file_as_it_was() {
        let dir = scratch("fails");
        fs::write(dir.join("a.rs"), "a").unwrap();
        let mut staged = Staged::default();
        staged.replace(dir.join("a.rs"), "ported a".into(), "a".into());
        staged.create(dir.join("new.toml"), "table".into());
        // A folder of that name cannot be replaced by a file.
        fs::create_dir(dir.join("b.rs")).unwrap();
        staged.replace(dir.join("b.rs"), "ported b".into(), String::new());
        let error = staged.commit().unwrap_err();
        assert!(error.ends_with("nothing was written"), "{error}");
        assert_eq!(fs::read_to_string(dir.join("a.rs")).unwrap(), "a");
        assert!(!dir.join("new.toml").exists());
        assert_eq!(
            fs::read_dir(&dir).unwrap().count(),
            2,
            "no temporary file left"
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_file_that_appeared_meanwhile_is_not_overwritten() {
        let dir = scratch("appeared");
        let mut staged = Staged::default();
        staged.create(dir.join("new.toml"), "table".into());
        fs::write(dir.join("new.toml"), "someone else's").unwrap();
        assert!(staged.commit().is_err());
        assert_eq!(
            fs::read_to_string(dir.join("new.toml")).unwrap(),
            "someone else's"
        );
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);
        fs::remove_dir_all(&dir).unwrap();
    }
}
