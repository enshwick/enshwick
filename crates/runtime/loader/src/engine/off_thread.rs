//! The loader's own threads: each is named for its work, a thread that
//! does not start is logged once, and reads that run once off the game
//! thread hand their result back, so the game thread never waits for the
//! archive or the disk.

use std::sync::Arc;
use std::time::Duration;

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
    not(windows),
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

    #[test]
    fn a_read_hands_its_value_and_a_panicking_read_its_failure_to_finish() {
        assert_eq!(finished(|| Ok(7)), Ok(7));
        assert_eq!(
            finished(|| panic!("a read that panics")),
            Err(Failed::Panicked)
        );
    }
}
