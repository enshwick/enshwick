//! Temporary folders, one per use, for commands and tests.

use std::ffi::OsStr;
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
        let _stale = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|error| format!("{}: {error}", dir.display()))?;
        Ok(Self(dir))
    }
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
        let _removed = std::fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
