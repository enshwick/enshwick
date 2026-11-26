//! Whole-file writes from a thread that must not stall, such as a client's
//! main thread, which draws the interface: a write waits about 30 ms for a
//! file another program holds, and while the file stays held its text is
//! kept and written again on a thread of its own until it lands. A later
//! write of the same file takes the place of one still waiting, readers see
//! the waiting text, and the process's end writes what is left.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use enshwick_base::__internal::held;
use enshwick_base::Relock;

/// How a file is written: [`whole`], or with what a caller adds around it,
/// such as the settings watch's note.
pub(crate) type Writer = fn(&Path, &str) -> io::Result<()>;

/// A write that waits for its file.
struct Waiting {
    path: PathBuf,
    text: String,
    write: Writer,
    /// Which write this is, so the writer never drops a newer text of the
    /// file that came while it wrote an older one.
    turn: u64,
}

/// The writes that wait, and whether a thread writes them.
struct Queue {
    waiting: Vec<Waiting>,
    turns: u64,
    writing: bool,
}

static QUEUE: Mutex<Queue> = Mutex::new(Queue {
    waiting: Vec::new(),
    turns: 0,
    writing: false,
});

/// The writer's first pause after a file is still held; it doubles up to
/// [`LONGEST_PAUSE`], since each try of a held file already waits a second.
const FIRST_PAUSE: Duration = Duration::from_millis(50);

/// The writer's longest pause between tries of a held file.
const LONGEST_PAUSE: Duration = Duration::from_secs(2);

/// Writes `text` to `path` whole, as
/// [`write_atomically`](enshwick_core::files::write_atomically) does.
///
/// # Errors
///
/// As `write_atomically`.
pub(crate) fn whole(path: &Path, text: &str) -> io::Result<()> {
    enshwick_core::files::write_atomically(path, text)
}

/// Writes `text` to `path` with `write`, waiting about 30 ms for a file
/// another handle holds; a file still held then gets the text later, from
/// a thread of its own. While an earlier write of `path` waits, this text
/// takes its place, so the files never go back to an older text.
///
/// # Errors
///
/// What `write` returns but a held file, whose text is kept.
pub(crate) fn write(path: &Path, text: &str, write: Writer) -> io::Result<()> {
    // Held across the try: a write of the same file from another thread
    // never lands under a text that waits.
    let mut queue = QUEUE.relock();
    queue.turns += 1;
    let turn = queue.turns;
    let kept = Waiting {
        path: path.to_owned(),
        text: text.to_owned(),
        write,
        turn,
    };
    if let Some(waiting) = queue.waiting.iter_mut().find(|w| w.path == path) {
        *waiting = kept;
        start(&mut queue);
        return Ok(());
    }
    match enshwick_base::__internal::briefly(|| write(path, text)) {
        Err(e) if held(&e) => crate::log!(
            WARN,
            "{} is held by another program ({e}); it is written once let go",
            path.display()
        ),
        done => return done,
    }
    queue.waiting.push(kept);
    start(&mut queue);
    Ok(())
}

/// The text of a write of `path` that waits for its file: a reader takes
/// it in place of what the file holds.
pub(crate) fn waiting(path: &Path) -> Option<String> {
    QUEUE
        .relock()
        .waiting
        .iter()
        .find(|w| w.path == path)
        .map(|w| w.text.clone())
}

/// Starts the writer unless it runs; a thread that does not start leaves
/// the texts for the next write or the process's end.
fn start(queue: &mut Queue) {
    at_exit::register();
    if queue.writing {
        return;
    }
    let spawned = std::thread::Builder::new()
        .name("enshwick held files".to_owned())
        .spawn(write_waiting);
    match spawned {
        Ok(_) => queue.writing = true,
        Err(e) => crate::log!(WARN, "held files wait for the next write: {e}"),
    }
}

/// The writer: each waiting text in turn, with the full wait for a held
/// file, until none waits.
fn write_waiting() {
    let mut pause = FIRST_PAUSE;
    loop {
        let (path, text, write, turn) = {
            let mut queue = QUEUE.relock();
            let Some(next) = queue.waiting.first() else {
                queue.writing = false;
                return;
            };
            (next.path.clone(), next.text.clone(), next.write, next.turn)
        };
        let written = std::panic::catch_unwind(|| write(&path, &text))
            .unwrap_or_else(|_| Err(io::Error::other("the write panicked")));
        let mut queue = QUEUE.relock();
        let at = queue.waiting.iter().position(|w| w.path == path);
        match written {
            Err(e) if held(&e) => {
                // Behind the others: one file held for long holds up no other.
                if let Some(at) = at {
                    let again = queue.waiting.remove(at);
                    queue.waiting.push(again);
                }
                drop(queue);
                std::thread::sleep(pause);
                pause = (pause * 2).min(LONGEST_PAUSE);
                continue;
            }
            Ok(()) => crate::log!(INFO, "{} written once let go", path.display()),
            Err(e) => crate::log!(WARN, "{} not written: {e}", path.display()),
        }
        if let Some(at) = at.filter(|&at| queue.waiting.get(at).is_some_and(|w| w.turn == turn)) {
            queue.waiting.remove(at);
        }
        pause = FIRST_PAUSE;
    }
}

/// Writes every text still waiting, with the full wait, as the process
/// ends. Every other thread is gone by then: a queue one of them held stays
/// held, so it gives up rather than wait.
#[cfg(any(test, windows))]
fn write_left() {
    let mut queue = match QUEUE.try_lock() {
        Ok(queue) => queue,
        Err(std::sync::TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
        Err(std::sync::TryLockError::WouldBlock) => return,
    };
    enshwick_base::__internal::wait_briefly_on_this_thread(false);
    for left in queue.waiting.drain(..) {
        // The process ends: nothing is left to tell, and the log may be gone.
        let _written = whole(&left.path, &left.text);
    }
}

#[cfg(all(windows, not(test)))]
mod at_exit {
    unsafe extern "C" {
        /// The C runtime's `atexit`; in a DLL its functions run after
        /// `DllMain` gets `DLL_PROCESS_DETACH`, so at an `ExitProcess`, not
        /// at a `TerminateProcess` (Microsoft's C runtime reference, atexit).
        fn atexit(function: extern "C" fn()) -> core::ffi::c_int;
    }

    /// Registers [`super::write_left`] for the process's end, once.
    pub(super) fn register() {
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(|| {
            // SAFETY: `at_end` is an `extern "C" fn()` of this DLL, which
            // stays loaded until the C runtime calls it at its detach; it
            // never unwinds.
            if unsafe { atexit(at_end) } != 0 {
                crate::log!(WARN, "held files are not written at the game's end");
            }
        });
    }

    extern "C" fn at_end() {
        let _ended = std::panic::catch_unwind(super::write_left);
    }
}

#[cfg(not(all(windows, not(test))))]
mod at_exit {
    /// Tests and other systems end the process with nothing to write.
    pub(super) const fn register() {}
}

#[cfg(all(test, windows))]
mod tests {
    use std::os::windows::fs::OpenOptionsExt as _;
    use std::time::Instant;

    use super::*;

    fn dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "enshwick-frame-writes-{name}-{}",
            std::process::id()
        ));
        let _gone = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// `path` held with no sharing at all, as a scanner holds a file it reads.
    fn hold(path: &Path) -> std::fs::File {
        std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(path)
            .unwrap()
    }

    fn landed(path: &Path, text: &str) -> bool {
        let started = Instant::now();
        while started.elapsed() < Duration::from_secs(10) {
            if waiting(path).is_none() && std::fs::read_to_string(path).is_ok_and(|t| t == text) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        false
    }

    #[test]
    fn a_held_file_stalls_the_writer_briefly_and_gets_the_latest_text_once_let_go() {
        let dir = dir("held");
        let path = dir.join("mod-controls.toml");
        std::fs::write(&path, "first").unwrap();
        let held = hold(&path);
        let started = Instant::now();
        write(&path, "second", whole).unwrap();
        write(&path, "third", whole).unwrap();
        assert!(
            started.elapsed() < Duration::from_millis(500),
            "the frame waits about 30 ms, not a second: {:?}",
            started.elapsed()
        );
        assert_eq!(waiting(&path).as_deref(), Some("third"), "readers see it");
        drop(held);
        assert!(landed(&path, "third"), "the latest text lands once let go");
        let _gone = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_free_file_is_written_at_once_and_a_failure_that_is_no_hold_is_returned() {
        let dir = dir("free");
        let path = dir.join("settings.toml");
        write(&path, "value = 1\n", whole).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "value = 1\n");
        assert_eq!(waiting(&path), None);
        let other = dir.join("other.toml");
        let refused = write(&other, "text", |_, _| Err(io::ErrorKind::NotFound.into()));
        assert_eq!(refused.unwrap_err().kind(), io::ErrorKind::NotFound);
        assert_eq!(
            waiting(&other),
            None,
            "nothing kept for a failure that is no hold"
        );
        let _gone = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_process_end_writes_what_still_waits() {
        let dir = dir("end");
        let path = dir.join("world.toml");
        std::fs::write(&path, "old").unwrap();
        {
            let mut queue = QUEUE.relock();
            queue.turns += 1;
            let turn = queue.turns;
            queue.waiting.push(Waiting {
                path: path.clone(),
                text: "new".to_owned(),
                write: whole,
                turn,
            });
        }
        write_left();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new");
        assert_eq!(waiting(&path), None);
        let _gone = std::fs::remove_dir_all(&dir);
    }
}
