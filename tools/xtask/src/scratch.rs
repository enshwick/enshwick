//! Temporary folders, one per use, for commands and tests.

use std::ffi::OsStr;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

/// A fresh, empty folder of its own for one use, removed when dropped.
pub(crate) struct Scratch(PathBuf);

impl Scratch {
    /// Makes a folder in the temp folder named by `purpose`, the process and
    /// a count, since tests run as threads of one process under `cargo test`
    /// and the process alone would give two of them one folder.
    pub(crate) fn new(purpose: &str) -> Result<Self, String> {
        static MADE: AtomicU32 = AtomicU32::new(0);
        let made = MADE.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("xtask-{purpose}-{}-{made}", std::process::id()));
        empty(&dir)?;
        Ok(Self(dir))
    }
}

/// Makes `dir` an empty folder. A leftover of an earlier process with the
/// same id that will not go is an error, not a folder to reuse.
fn empty(dir: &Path) -> Result<(), String> {
    match std::fs::remove_dir_all(dir) {
        Err(error) if error.kind() != ErrorKind::NotFound => {
            return Err(format!("{}: a stale folder stays: {error}", dir.display()));
        }
        _ => {}
    }
    std::fs::create_dir_all(dir).map_err(|error| format!("{}: {error}", dir.display()))
}

impl core::ops::Deref for Scratch {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.0
    }
}

impl AsRef<Path> for Scratch {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

impl AsRef<OsStr> for Scratch {
    fn as_ref(&self) -> &OsStr {
        self.0.as_os_str()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let removed = std::fs::remove_dir_all(&self.0);
        // A command leaves a folder that will not go to `wsx clean`; a test
        // fails on it, since a handle its code leaked is the usual cause.
        #[cfg(test)]
        tests::gone(&self.0, &removed);
        drop(removed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fails a test whose folder stayed, unless it already fails.
    pub(super) fn gone(dir: &Path, removed: &std::io::Result<()>) {
        if let Err(error) = removed {
            assert!(
                std::thread::panicking() || error.kind() == ErrorKind::NotFound,
                "{} stayed: {error}",
                dir.display()
            );
        }
    }

    /// Tests of one binary share a process under `cargo test`, so a second
    /// folder of the same purpose must not remove the first; each goes when
    /// its use ends, however it ends.
    #[test]
    fn a_scratch_folder_is_its_own_even_within_one_process_and_goes_with_its_use() {
        let first = Scratch::new("test-apart").unwrap();
        let kept = first.join("kept");
        std::fs::write(&kept, "").unwrap();
        let second = Scratch::new("test-apart").unwrap();
        assert_ne!(first.to_path_buf(), second.to_path_buf());
        assert!(kept.exists(), "the second folder removed the first");
        let paths = [first.to_path_buf(), second.to_path_buf()];
        drop((first, second));
        for path in paths {
            assert!(!path.exists(), "{} stayed", path.display());
        }
    }

    /// A reused process id finds the folder of an earlier run: its files go,
    /// and a file in it that cannot go refuses the folder.
    #[test]
    fn a_stale_folder_is_emptied_and_one_that_will_not_go_is_refused() {
        let root = Scratch::new("test-stale").unwrap();
        let stale = root.join("stale");
        std::fs::create_dir_all(&stale).unwrap();
        std::fs::write(stale.join("old"), "").unwrap();
        empty(&stale).unwrap();
        assert_eq!(std::fs::read_dir(&stale).unwrap().count(), 0);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt as _;
            let held = std::fs::OpenOptions::new()
                .create(true)
                .truncate(true)
                .write(true)
                .share_mode(0)
                .open(stale.join("held"))
                .unwrap();
            let refused = empty(&stale).unwrap_err();
            assert!(refused.contains("a stale folder stays"), "{refused}");
            drop(held);
        }
    }
}
