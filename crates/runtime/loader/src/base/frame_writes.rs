//! Whole-file writes from a thread that must not stall, such as a client's
//! main thread, which draws the interface: a write waits about 30 ms for a
//! file another program holds, and while the file stays held its text is
//! kept and written again on a thread of its own until it lands. A later
//! write of the same file takes the place of one still waiting, readers see
//! the waiting text, and the process's end writes what is left.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use arc_swap::ArcSwapOption;
use enshwick_base::__internal::{briefly, held};
use enshwick_base::Relock;

/// How a file is written: [`whole`], or with what a caller adds around it,
/// such as the settings watch's note. The process's end writes a text with
/// [`whole`] alone, since a lock the writer takes may be held by a thread
/// the end already stopped.
pub(crate) type Writer = fn(&Path, &str) -> io::Result<()>;

/// A write that waits for its file.
struct Waiting {
    path: PathBuf,
    text: String,
    write: Writer,
    /// Which write this is, so the writer never drops a newer text of the
    /// file that came while it wrote an older one.
    turn: u64,
    /// The file's modified time when the text came or the loader last wrote
    /// it: a program that writes the file after letting it go wins.
    seen: Option<SystemTime>,
    /// When the text came, to give up on a file that never lets go.
    since: Instant,
}

/// A file whose kept text the writer gave up on: its readers went back to
/// what the file holds, and whom the loader told so far.
struct Lost {
    path: PathBuf,
    /// The settings watch raised the change of the mod whose file it is.
    raised: bool,
    /// The administrators online were told, on a server's game thread.
    told: bool,
}

/// The writes that wait, and whether a thread writes them.
struct Queue {
    waiting: Vec<Waiting>,
    turns: u64,
    writing: bool,
    /// The files given up on, the newest last, at most [`MOST_LOST`].
    lost: Vec<Lost>,
    /// The files given up on whose readers have not read them again yet.
    unread: Vec<PathBuf>,
}

/// The queue, and whether anything waits or was given up on, so readers
/// skip its lock while nothing does.
struct State {
    queue: Mutex<Queue>,
    any: AtomicBool,
    any_lost: AtomicBool,
    /// Some given-up file waits to be told to the administrators.
    untold: AtomicBool,
    /// The given-up files as last changed, for the Mods screen, which reads
    /// them each frame and must not wait for a write that holds the queue.
    shown: ArcSwapOption<Vec<PathBuf>>,
    /// The owners that keep a file's content in memory, which read it again
    /// once a write of it is given up on.
    readers: Mutex<Vec<Reread>>,
}

/// Reads a file again after the writer gave up on the text kept for it, so
/// an owner that keeps the file's content in memory goes back to what the
/// file holds; it ignores a file that is not its own.
pub(crate) type Reread = fn(&Path);

/// How many given-up files the loader remembers to show; a file given up
/// on again moves to the end instead of taking a second place.
const MOST_LOST: usize = 8;

static STATE: State = State::new();

/// The writer's first pause after a file is still held; it doubles up to
/// [`LONGEST_PAUSE`].
const FIRST_PAUSE: Duration = Duration::from_millis(50);

/// The writer's longest pause between tries of a held file.
const LONGEST_PAUSE: Duration = Duration::from_secs(2);

/// How long the writer tries a text before it gives up: a file held or
/// refused that long is no short hold of a scanner.
const GIVE_UP_AFTER: Duration = Duration::from_mins(10);

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
/// What `write` returns but a held file, whose text is kept; a read-only
/// file, which Windows also refuses as if held, returns its error.
pub(crate) fn write(path: &Path, text: &str, write: Writer) -> io::Result<()> {
    // Held across the try: a write of the same file from another thread
    // never lands under a text that waits.
    let mut queue = STATE.queue.relock();
    queue.turns += 1;
    let kept = Waiting {
        path: path.to_owned(),
        text: text.to_owned(),
        write,
        turn: queue.turns,
        seen: modified(path),
        since: Instant::now(),
    };
    if let Some(waiting) = queue.waiting.iter_mut().find(|w| w.path == path) {
        *waiting = kept;
        start(&mut queue);
        return Ok(());
    }
    match briefly(|| write(path, text)) {
        Err(e) if held(&e) && !read_only(path) => crate::log!(
            WARN,
            "{} is held by another program ({e}); it is written once let go",
            path.display()
        ),
        done => {
            if done.is_ok() {
                STATE.found(&mut queue, path);
            }
            return done;
        }
    }
    queue.waiting.push(kept);
    STATE.any.store(true, Ordering::Release);
    start(&mut queue);
    Ok(())
}

/// The text of a write of `path` that waits for its file: a reader takes
/// it in place of what the file holds.
pub(crate) fn waiting(path: &Path) -> Option<String> {
    STATE.waiting(path)
}

/// The files whose kept text was given up on, the newest last, for the Mods
/// screen; read without the queue's lock.
pub(crate) fn lost() -> Option<Arc<Vec<PathBuf>>> {
    STATE.shown.load_full()
}

/// The given-up files the settings watch has not raised yet; each comes
/// once.
pub(crate) fn unraised() -> Vec<PathBuf> {
    STATE.lost(|lost| &mut lost.raised)
}

/// Names `reread`, for an owner above base when it seals. It runs on the
/// writer's thread with each file given up on, outside the queue's lock, so
/// it takes no game lock.
pub(crate) fn on_give_up(reread: Reread) {
    STATE.readers.relock().push(reread);
}

/// After a server step on its game thread: tells each administrator online
/// of every file given up on since, by name; with none online the files
/// wait for the first step one is.
#[cfg(windows)]
pub(crate) fn after_step(loader: &crate::engine::loader::Loader) {
    if !STATE.untold.load(Ordering::Acquire) {
        return;
    }
    let Ok(players) = crate::engine::world::players(loader) else {
        return;
    };
    let admins: Vec<_> = players
        .iter()
        .filter(|p| p.is_admin())
        .map(|p| p.key.get())
        .collect();
    STATE.tell(&admins, |key, path| {
        let text = crate::base::net::for_player(key, || lost_line(path));
        crate::base::tell::tell(loader, key, &text);
    });
}

/// The words that tell a player the change to `path` is lost.
pub(crate) fn lost_line(path: &Path) -> String {
    crate::engine::texts::fill(
        "files.lost",
        &[
            ("path", &path.display().to_string()),
            ("minutes", &(GIVE_UP_AFTER.as_secs() / 60).to_string()),
        ],
    )
}

/// Starts the writer unless it runs; a thread that does not start leaves
/// the texts for the next write or the process's end.
fn start(queue: &mut Queue) {
    at_exit::register();
    if queue.writing {
        return;
    }
    match crate::engine::off_thread::spawn("held files", write_waiting) {
        Ok(()) => queue.writing = true,
        Err(e) => crate::log!(WARN, "held files wait for the next write: {e}"),
    }
}

/// The writer: each waiting text in turn, each try waiting about 30 ms
/// for a held file, until none waits.
fn write_waiting() {
    let mut pause = FIRST_PAUSE;
    while let Some((path, text, write, turn)) = STATE.next() {
        // Brief tries keep the locks a writer takes, such as the settings
        // watch's, short; the pauses between them do the waiting.
        let written = std::panic::catch_unwind(|| briefly(|| write(&path, &text)))
            .unwrap_or_else(|_| Err(io::Error::other("the write panicked")));
        let go_on = STATE.settle(&path, turn, written);
        STATE.reread();
        if go_on {
            pause = FIRST_PAUSE;
        } else {
            std::thread::sleep(pause);
            pause = (pause * 2).min(LONGEST_PAUSE);
        }
    }
}

/// The file's modified time, `None` when it does not read.
fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// Whether `path` is marked read-only, which no wait lets go.
fn read_only(path: &Path) -> bool {
    std::fs::metadata(path).is_ok_and(|m| m.permissions().readonly())
}

/// Whether another program wrote the file of `waiting` since its text
/// came; an unknown time on either side counts as no change.
fn changed(waiting: &Waiting) -> bool {
    waiting
        .seen
        .zip(modified(&waiting.path))
        .is_some_and(|(seen, now)| seen != now)
}

impl State {
    const fn new() -> Self {
        Self {
            queue: Mutex::new(Queue {
                waiting: Vec::new(),
                turns: 0,
                writing: false,
                lost: Vec::new(),
                unread: Vec::new(),
            }),
            any: AtomicBool::new(false),
            any_lost: AtomicBool::new(false),
            untold: AtomicBool::new(false),
            shown: ArcSwapOption::const_empty(),
            readers: Mutex::new(Vec::new()),
        }
    }

    /// Runs every named reader with each file given up on since, outside
    /// the queue's lock; a reader that panics leaves the others to run.
    fn reread(&self) {
        let paths = std::mem::take(&mut self.queue.relock().unread);
        if paths.is_empty() {
            return;
        }
        let readers = self.readers.relock().clone();
        for path in &paths {
            for reread in &readers {
                if std::panic::catch_unwind(|| reread(path)).is_err() {
                    crate::log!(
                        WARN,
                        "{} was not read again after its change was given up",
                        path.display()
                    );
                }
            }
        }
    }

    /// The given-up files whose flag `pick` names is unset, which it then
    /// sets.
    fn lost(&self, pick: impl Fn(&mut Lost) -> &mut bool) -> Vec<PathBuf> {
        if !self.any_lost.load(Ordering::Acquire) {
            return Vec::new();
        }
        let mut queue = self.queue.relock();
        queue
            .lost
            .iter_mut()
            .filter_map(|lost| {
                let done = pick(lost);
                (!std::mem::replace(done, true)).then(|| lost.path.clone())
            })
            .collect()
    }

    /// Sends each of `admins` every given-up file they were not told of;
    /// with no administrator the files stay untold.
    fn tell<T: Copy>(&self, admins: &[T], mut send: impl FnMut(T, &Path)) {
        if admins.is_empty() {
            return;
        }
        // Cleared before the files are taken, so a file given up meanwhile
        // sets it again and waits for the next step.
        self.untold.store(false, Ordering::Release);
        for path in self.lost(|lost| &mut lost.told) {
            for &admin in admins {
                send(admin, &path);
            }
        }
    }

    /// Publishes the given-up files for the Mods screen, under the queue's
    /// lock.
    fn show(&self, queue: &Queue) {
        let paths: Vec<PathBuf> = queue.lost.iter().map(|lost| lost.path.clone()).collect();
        self.shown
            .store((!paths.is_empty()).then(|| Arc::new(paths)));
    }

    /// Forgets that the writer gave up on `path`, which a write just reached.
    fn found(&self, queue: &mut Queue, path: &Path) {
        if self.any_lost.load(Ordering::Acquire) {
            queue.lost.retain(|lost| lost.path != path);
            self.any_lost
                .store(!queue.lost.is_empty(), Ordering::Release);
            self.show(queue);
        }
    }

    /// Remembers that the writer gave up on `path`, under the queue's lock.
    fn give_up(&self, queue: &mut Queue, path: PathBuf) {
        queue.lost.retain(|lost| lost.path != path);
        if queue.lost.len() == MOST_LOST {
            queue.lost.remove(0);
        }
        queue.unread.push(path.clone());
        queue.lost.push(Lost {
            path,
            raised: false,
            told: false,
        });
        self.any_lost.store(true, Ordering::Release);
        self.untold.store(true, Ordering::Release);
        self.show(queue);
    }

    /// The text that waits for `path`, unless another program wrote the
    /// file since.
    fn waiting(&self, path: &Path) -> Option<String> {
        if !self.any.load(Ordering::Acquire) {
            return None;
        }
        self.queue
            .relock()
            .waiting
            .iter()
            .find(|w| w.path == path && !changed(w))
            .map(|w| w.text.clone())
    }

    /// The next text for the writer, dropping those whose file another
    /// program wrote since; `None` ends the writer.
    fn next(&self) -> Option<(PathBuf, String, Writer, u64)> {
        let mut queue = self.queue.relock();
        loop {
            let Some(first) = queue.waiting.first() else {
                queue.writing = false;
                self.any.store(false, Ordering::Release);
                return None;
            };
            if !changed(first) {
                return Some((
                    first.path.clone(),
                    first.text.clone(),
                    first.write,
                    first.turn,
                ));
            }
            let gone = queue.waiting.remove(0);
            crate::log!(
                INFO,
                "{} was written by another program once let go; its text stays",
                gone.path.display()
            );
        }
    }

    /// Takes the result of the writer's try of the text `turn` of `path`;
    /// whether the writer goes on at once, else it pauses first.
    fn settle(&self, path: &Path, turn: u64, written: io::Result<()>) -> bool {
        let mut queue = self.queue.relock();
        let at = queue.waiting.iter().position(|w| w.path == path);
        let go_on = match (written, at) {
            (_, None) => true,
            (Ok(()), Some(at)) => {
                crate::log!(INFO, "{} written once let go", path.display());
                self.found(&mut queue, path);
                if let Some(newer) = queue.waiting.get_mut(at).filter(|w| w.turn != turn) {
                    newer.seen = modified(path);
                } else {
                    queue.waiting.remove(at);
                }
                true
            }
            (Err(e), Some(at)) => {
                let kept = queue.waiting.remove(at);
                if kept.since.elapsed() < GIVE_UP_AFTER {
                    // Behind the others: one file held for long holds up no other.
                    queue.waiting.push(kept);
                    false
                } else {
                    crate::log!(
                        WARN,
                        "{} not written, given up after {} minutes: {e}",
                        path.display(),
                        GIVE_UP_AFTER.as_secs() / 60
                    );
                    self.give_up(&mut queue, kept.path);
                    true
                }
            }
        };
        self.any.store(!queue.waiting.is_empty(), Ordering::Release);
        go_on
    }

    /// Writes every text still waiting, with the full wait, as the process
    /// ends. Every other thread is gone by then: a queue one of them held
    /// stays held, so it gives up rather than wait.
    #[cfg(any(test, windows))]
    fn write_left(&self) {
        let mut queue = match self.queue.try_lock() {
            Ok(queue) => queue,
            Err(std::sync::TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
            Err(std::sync::TryLockError::WouldBlock) => return,
        };
        enshwick_base::__internal::wait_briefly_on_this_thread(false);
        for left in queue.waiting.drain(..) {
            // The process ends: nothing is left to tell, and the log may be gone.
            let _written = whole(&left.path, &left.text);
        }
        self.any.store(false, Ordering::Release);
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

    /// Registers the writes of what is left for the process's end, once.
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
        let _ended = std::panic::catch_unwind(|| super::STATE.write_left());
    }
}

#[cfg(not(all(windows, not(test))))]
mod at_exit {
    /// Tests and other systems end the process with nothing to write.
    pub(super) const fn register() {}
}

#[cfg(test)]
#[cfg(windows)]
mod tests {
    use std::os::windows::fs::OpenOptionsExt as _;

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

    /// A text of `path` queued in `state` as `write` keeps it.
    fn queue(state: &State, path: &Path, text: &str, since: Instant) -> u64 {
        let mut queue = state.queue.relock();
        queue.turns += 1;
        let turn = queue.turns;
        queue.waiting.push(Waiting {
            path: path.to_owned(),
            text: text.to_owned(),
            write: whole,
            turn,
            seen: modified(path),
            since,
        });
        state.any.store(true, Ordering::Release);
        turn
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
    fn a_read_only_file_returns_its_refusal_and_keeps_nothing() {
        let dir = dir("read-only");
        let path = dir.join("mod-controls.toml");
        std::fs::write(&path, "old").unwrap();
        let mut permissions = std::fs::metadata(&path).unwrap().permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&path, permissions.clone()).unwrap();
        let refused = write(&path, "new", whole);
        #[expect(
            clippy::permissions_set_readonly_false,
            reason = "the test file is removed after"
        )]
        permissions.set_readonly(false);
        std::fs::set_permissions(&path, permissions).unwrap();
        assert_eq!(refused.unwrap_err().kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(waiting(&path), None, "a lasting refusal is no hold");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "old");
        let _gone = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_failed_try_keeps_its_text_until_the_writer_gives_up() {
        let state = State::new();
        let dir = dir("give-up");
        let path = dir.join("settings.toml");
        let turn = queue(&state, &path, "kept", Instant::now());
        let full = || Err(io::Error::other("the disk is full"));
        assert!(!state.settle(&path, turn, full()), "the writer pauses");
        assert_eq!(state.waiting(&path).as_deref(), Some("kept"));
        state.queue.relock().waiting.clear();
        let long_ago = Instant::now().checked_sub(GIVE_UP_AFTER).unwrap();
        let turn = queue(&state, &path, "old", long_ago);
        assert!(state.settle(&path, turn, full()), "the writer goes on");
        assert_eq!(state.waiting(&path), None, "given up");
        assert!(state.next().is_none(), "the writer ends");
        let _gone = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_given_up_file_is_raised_and_told_once_and_shown_until_a_write_lands() {
        let state = State::new();
        let dir = dir("lost");
        let path = dir.join("settings.toml");
        let long_ago = Instant::now().checked_sub(GIVE_UP_AFTER).unwrap();
        let full = || Err(io::Error::other("the disk is full"));
        assert!(shown(&state).is_empty(), "nothing lost yet");
        let turn = queue(&state, &path, "kept", long_ago);
        assert!(state.settle(&path, turn, full()));
        let raised = |state: &State| state.lost(|lost| &mut lost.raised);
        assert_eq!(
            raised(&state),
            std::slice::from_ref(&path),
            "the watch raises it"
        );
        assert!(raised(&state).is_empty(), "once");
        assert_eq!(
            told(&state, &[1]),
            [(1, path.clone())],
            "the admins hear of it apart"
        );
        assert!(told(&state, &[1]).is_empty(), "once");
        assert_eq!(
            shown(&state),
            std::slice::from_ref(&path),
            "the Mods screen keeps it"
        );
        let turn = queue(&state, &path, "again", long_ago);
        assert!(state.settle(&path, turn, full()));
        assert_eq!(shown(&state).len(), 1, "a file lost again takes one place");
        assert_eq!(
            raised(&state),
            std::slice::from_ref(&path),
            "and is raised again"
        );
        let turn = queue(&state, &path, "landed", Instant::now());
        assert!(state.settle(&path, turn, Ok(())));
        assert!(shown(&state).is_empty(), "a write that lands clears it");
        let _gone = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_loader_remembers_only_the_latest_given_up_files() {
        let state = State::new();
        let mut queue = state.queue.relock();
        for at in 0..=MOST_LOST {
            state.give_up(&mut queue, PathBuf::from(format!("{at}.toml")));
        }
        drop(queue);
        let lost = shown(&state);
        assert_eq!(lost.len(), MOST_LOST);
        assert_eq!(lost[0], PathBuf::from("1.toml"), "the oldest goes");
    }

    #[test]
    fn a_given_up_file_waits_for_an_administrator_to_be_told() {
        let state = State::new();
        let path = PathBuf::from("mutes.toml");
        state.give_up(&mut state.queue.relock(), path.clone());
        assert!(told(&state, &[]).is_empty(), "no administrator online");
        assert!(state.untold.load(Ordering::Acquire), "it still waits");
        assert_eq!(
            told(&state, &[1, 2]),
            [(1, path.clone()), (2, path)],
            "each administrator who comes hears it"
        );
        assert!(!state.untold.load(Ordering::Acquire));
        assert!(told(&state, &[3]).is_empty(), "once");
    }

    #[test]
    fn each_named_reader_reads_a_given_up_file_again_once() {
        static READ: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());
        fn record(path: &Path) {
            READ.relock().push(path.to_owned());
        }
        fn panics(_: &Path) {
            panic!("a reader's fault");
        }
        let state = State::new();
        state.readers.relock().extend([panics as Reread, record]);
        state.reread();
        assert!(READ.relock().is_empty(), "nothing given up yet");
        let path = PathBuf::from("mutes.toml");
        state.give_up(&mut state.queue.relock(), path.clone());
        state.reread();
        state.reread();
        assert_eq!(*READ.relock(), [path], "once, past a reader that panics");
    }

    #[test]
    fn the_mods_screen_reads_the_given_up_files_while_a_write_holds_the_queue() {
        let state = State::new();
        let mut queue = state.queue.relock();
        state.give_up(&mut queue, PathBuf::from("controls.toml"));
        assert_eq!(shown(&state), [PathBuf::from("controls.toml")]);
        drop(queue);
    }

    /// The given-up files the Mods screen shows.
    fn shown(state: &State) -> Vec<PathBuf> {
        state
            .shown
            .load_full()
            .map_or_else(Vec::new, |paths| paths.to_vec())
    }

    /// Whom of `admins` [`State::tell`] tells of which file.
    fn told(state: &State, admins: &[u32]) -> Vec<(u32, PathBuf)> {
        let mut sent = Vec::new();
        state.tell(admins, |admin, path| sent.push((admin, path.to_owned())));
        sent
    }

    #[test]
    fn a_file_another_program_writes_after_letting_go_keeps_its_text() {
        let state = State::new();
        let dir = dir("edited");
        let path = dir.join("settings.toml");
        std::fs::write(&path, "old").unwrap();
        queue(&state, &path, "queued", Instant::now());
        state.queue.relock().waiting[0].seen = Some(SystemTime::UNIX_EPOCH);
        assert_eq!(state.waiting(&path), None, "readers see the edit");
        assert!(state.next().is_none(), "the writer drops the text");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "old");
        let _gone = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_older_text_that_lands_leaves_the_newer_one_waiting() {
        let state = State::new();
        let dir = dir("newer");
        let path = dir.join("mod-controls.toml");
        std::fs::write(&path, "old").unwrap();
        let older = queue(&state, &path, "older", Instant::now());
        {
            let mut queue = state.queue.relock();
            queue.turns += 1;
            let newer = queue.turns;
            let waiting = &mut queue.waiting[0];
            waiting.text = "newer".to_owned();
            waiting.turn = newer;
        }
        std::fs::write(&path, "older").unwrap();
        assert!(state.settle(&path, older, Ok(())));
        assert_eq!(
            state.waiting(&path).as_deref(),
            Some("newer"),
            "the loader's own write is no other program's"
        );
        let _gone = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_process_end_writes_what_still_waits() {
        let state = State::new();
        let dir = dir("end");
        let path = dir.join("world.toml");
        std::fs::write(&path, "old").unwrap();
        queue(&state, &path, "new", Instant::now());
        state.write_left();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new");
        assert_eq!(state.waiting(&path), None);
        let _gone = std::fs::remove_dir_all(&dir);
    }
}
