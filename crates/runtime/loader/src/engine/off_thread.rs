//! The loader's own threads: each is named for its work, a thread that
//! does not start is logged once, and reads that run once off the game
//! thread hand their result back, so the game thread never waits for the
//! archive or the disk.

use std::sync::Arc;
use std::time::Duration;

/// The system's id of the calling thread, which the loader compares to tell
/// the game's threads apart.
pub(crate) fn current_thread() -> u32 {
    // SAFETY: `GetCurrentThreadId` takes no arguments and cannot fail.
    unsafe { windows_sys::Win32::System::Threading::GetCurrentThreadId() }
}

/// Starts `work` on the thread `enshwick {what}`; false, with a warning
/// naming `what`, when no thread starts.
pub(crate) fn start(what: &str, work: impl FnOnce() + Send + 'static) -> bool {
    match spawn(what, work) {
        Ok(()) => true,
        Err(why) => {
            crate::log!(WARN, "{what}: no thread to run it ({why})");
            false
        }
    }
}

/// Starts `work` on the thread `enshwick {what}`, for a caller that tells
/// of a thread that does not start its own way, such as the log's writer.
///
/// # Errors
///
/// The system's error when no thread starts.
pub(crate) fn spawn(what: &str, work: impl FnOnce() + Send + 'static) -> std::io::Result<()> {
    std::thread::Builder::new()
        .name(format!("enshwick {what}"))
        .spawn(crate::engine::per_test::carried(work))
        .map(drop)
}

/// The failures every read off the game thread shares.
pub(crate) trait ReadFailure: core::fmt::Display + Send + 'static {
    /// The read panicked.
    const PANICKED: Self;
    /// No thread could be started.
    const THREAD: Self;
}

/// Runs `read` on the thread `enshwick {what}` and hands `finish` what it
/// read and how long it took; a panic in `read` is [`ReadFailure::PANICKED`].
/// When no thread starts, `finish` gets [`ReadFailure::THREAD`] on the
/// calling thread. `finish` runs once either way.
#[cfg_attr(
    all(not(windows), not(test)),
    expect(dead_code, reason = "the reads off the game thread run on Windows")
)]
pub(crate) fn read<T: Send + 'static, E: ReadFailure>(
    what: &'static str,
    read: impl FnOnce() -> Result<T, E> + Send + 'static,
    finish: impl Fn(Result<T, E>, Duration) + Send + Sync + 'static,
) {
    let finish = Arc::new(finish);
    let on_thread = Arc::clone(&finish);
    let started = start(what, move || {
        let began = std::time::Instant::now();
        let found = std::panic::catch_unwind(std::panic::AssertUnwindSafe(read))
            .unwrap_or(Err(E::PANICKED));
        on_thread(found, began.elapsed());
    });
    if !started {
        finish(Err(E::THREAD), Duration::ZERO);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use super::*;

    #[derive(Debug, PartialEq, Eq)]
    enum Failed {
        Panicked,
        Thread,
    }

    impl core::fmt::Display for Failed {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            write!(f, "{self:?}")
        }
    }

    impl ReadFailure for Failed {
        const PANICKED: Self = Self::Panicked;
        const THREAD: Self = Self::Thread;
    }

    fn finished(
        work: impl FnOnce() -> Result<u32, Failed> + Send + 'static,
    ) -> Result<u32, Failed> {
        let (sent, got) = mpsc::channel();
        let sent = std::sync::Mutex::new(sent);
        read("test read", work, move |found, _took| {
            sent.lock().unwrap().send(found).unwrap();
        });
        got.recv_timeout(Duration::from_secs(10)).unwrap()
    }

    /// The loader's code outside its tests, file by file, below `src`.
    fn shipped_sources() -> Vec<(std::path::PathBuf, String)> {
        let mut dirs = vec![std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src")];
        let mut files = Vec::new();
        while let Some(dir) = dirs.pop() {
            for path in std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()) {
                let name = path.file_name().unwrap().to_string_lossy().into_owned();
                if path.is_dir() {
                    dirs.push(path);
                } else if path.extension().is_some_and(|e| e == "rs") && name != "tests.rs" {
                    let text = std::fs::read_to_string(&path).unwrap();
                    let shipped = match text.find("#[cfg(test)]\nmod tests {") {
                        Some(at) => text[..at].to_owned(),
                        None => text,
                    };
                    files.push((path, shipped));
                }
            }
        }
        files
    }

    /// A thread this helper does not start runs in the loader's tests with
    /// a fuse scope of its own, so a test that drives it would not see what
    /// it turned off.
    #[test]
    fn every_thread_the_loader_starts_goes_through_this_helper() {
        let starts: Vec<_> = shipped_sources()
            .into_iter()
            .filter(|(path, _)| !path.ends_with("off_thread.rs"))
            .filter(|(_, text)| {
                let text = text.replace("off_thread::spawn(", "");
                text.contains("thread::spawn(") || text.contains("thread::Builder")
            })
            .map(|(path, _)| path)
            .collect();
        assert!(
            starts.is_empty(),
            "start threads with off_thread: {starts:?}"
        );
    }

    #[test]
    fn a_fuse_blown_on_a_started_thread_is_off_for_the_test_that_started_it() {
        static LAMP: crate::engine::fuse::Fuse = crate::engine::fuse::Fuse::new("started lamp");
        let (done, ran) = mpsc::channel();
        assert!(start("test lamp", move || {
            LAMP.blow("a started thread");
            done.send(()).unwrap();
        }));
        ran.recv_timeout(Duration::from_secs(10)).unwrap();
        assert!(LAMP.is_off());
    }

    #[test]
    fn a_read_hands_its_value_and_a_panicking_read_its_failure_to_finish() {
        assert_eq!(finished(|| Ok(7)), Ok(7));
        assert_eq!(
            finished(|| panic!("a read that panics")),
            Err(Failed::Panicked)
        );
    }
}
