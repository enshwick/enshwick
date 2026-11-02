//! `logs/enshwick/` (`all.log`, `loader.log`, `mods/<id>.log`): one file for the loader and every mod.
//!
//! Callers only format a line and send it down a bounded channel; a
//! background thread owns the files, so a hook never waits on the disk. A
//! line the full channel cannot take is counted and the count logged once
//! there is room. The writer also keeps the latest lines in memory for
//! `enshwick log` on the console. The writer keeps the two runs before as `<name>.1.log`
//! and `<name>.2.log`, each cut to [`KEPT_BYTES`]. A file that would pass its
//! cap during a run ([`ALL_CAP`], [`SOURCE_CAP`]) moves aside whole as run 1
//! the same way, so each log holds at most three times its cap.

use std::collections::VecDeque;
use std::fmt::Write as _;
use std::io::Write as _;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{SyncSender, TrySendError, sync_channel};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use arc_swap::ArcSwapOption;
use enshwick::abi::Level;

use crate::locks::Relock;
use crate::settings::{LOADER_ID, Settings};

/// The folder beside the game's executable that holds every log, each
/// writer's in a folder of its own.
pub const LOGS: &str = "logs";
/// Lines waiting for the writer at most.
const QUEUE: usize = 4096;
/// The latest lines kept in memory.
const RECENT: usize = 1024;
/// How often [`settle`] looks at the writer.
const SETTLE_POLL: Duration = Duration::from_millis(2);
/// The most bytes kept of one earlier run's file: its last ones.
const KEPT_BYTES: u64 = 8 << 20;
/// Earlier runs kept beside the current one.
const KEPT_RUNS: usize = 2;
/// The most bytes `all.log` holds before it rolls over.
const ALL_CAP: u64 = 32 << 20;
/// The most bytes `loader.log` or a mod's file holds before it rolls over.
const SOURCE_CAP: u64 = 8 << 20;

/// One line as written, with its level and where its source sits in it, so
/// the source costs no copy of its own.
#[derive(Debug)]
struct Line {
    level: Level,
    text: String,
    source: Range<usize>,
}

impl Line {
    fn new(millis: u64, level: Level, source: &str, message: &str) -> Self {
        let secs = millis / 1000;
        let (h, m, s) = (secs / 3600 % 24, secs / 60 % 60, secs % 60);
        let name = level.name();
        let mut text = String::with_capacity(32 + source.len() + message.len());
        let _time = write!(
            text,
            "{h:02}:{m:02}:{s:02}.{:03}Z {name:<5} [",
            millis % 1000
        );
        let start = text.len();
        text.push_str(source);
        let source = start..text.len();
        let _message = writeln!(text, "] {message}");
        Self {
            level,
            text,
            source,
        }
    }

    fn source(&self) -> &str {
        self.text.get(self.source.clone()).unwrap_or_default()
    }
}

static LINES: OnceLock<SyncSender<Line>> = OnceLock::new();
/// What the channel to the writer has carried.
struct Counts {
    /// Lines the full channel could not take since the writer last said so.
    dropped: AtomicU64,
    /// Lines sent to the writer, for [`settle`].
    sent: AtomicU64,
    /// Lines the writer has kept among the latest, for [`settle`].
    taken: AtomicU64,
    /// Whether the writer stopped because its file could not be made.
    stopped: AtomicBool,
}

static COUNTS: Counts = Counts {
    dropped: AtomicU64::new(0),
    sent: AtomicU64::new(0),
    taken: AtomicU64::new(0),
    stopped: AtomicBool::new(false),
};
/// The latest lines the writer kept.
static RECENT_LINES: Mutex<Recent> = Mutex::new(Recent::new());
static LEVELS: ArcSwapOption<Settings> = ArcSwapOption::const_empty();

/// Keeps lines up to the levels in `settings`; before the first call every
/// source keeps `info`.
pub fn set_levels(settings: Settings) {
    LEVELS.store(Some(std::sync::Arc::new(settings)));
}

/// The most detailed level kept for lines from `source`.
#[must_use]
pub fn level(source: &str) -> Level {
    LEVELS
        .load()
        .as_ref()
        .map_or(Level::INFO, |s| s.level(source))
}

/// Whether a line at `level` from `source` would be kept, so a caller can
/// skip formatting one that would not.
#[must_use]
pub fn enabled(level: Level, source: &str) -> bool {
    level <= self::level(source)
}

/// Whether the writer runs, so lines reach the files.
#[must_use]
pub fn is_open() -> bool {
    LINES.get().is_some() && !COUNTS.stopped.load(Ordering::Acquire)
}

/// Starts the writer in `dir`, the previous runs' files moved aside first:
/// `all.log` gets every line, `loader.log` the loader's, `mods/<id>.log` each
/// mod's. When it cannot start, [`crate::early`] says why.
pub fn open(dir: &Path) {
    let mods = dir.join("mods");
    if let Err(e) = std::fs::create_dir_all(&mods) {
        crate::early::note(&format!(
            "no log: {} could not be made: {e}",
            mods.display()
        ));
        return;
    }
    let dir = dir.to_path_buf();
    let (send, receive) = sync_channel::<Line>(QUEUE);
    let started = std::thread::Builder::new()
        .name("enshwick-log".into())
        .spawn(move || {
            // On the writer's thread: the game's start never waits on the
            // disk, and lines sent meanwhile wait in the channel.
            rotate(&dir.join("all.log"));
            rotate(&dir.join("loader.log"));
            rotate_dir(&mods);
            let mut all = match Capped::create(dir.join("all.log"), ALL_CAP) {
                Ok(all) => all,
                Err(e) => {
                    COUNTS.stopped.store(true, Ordering::Release);
                    crate::early::write(&format!(
                        "no log: {} could not be written: {e}",
                        dir.join("all.log").display()
                    ));
                    return;
                }
            };
            let mut files: rustc_hash::FxHashMap<String, Option<Capped>> =
                rustc_hash::FxHashMap::default();
            for line in receive {
                let dropped = COUNTS.dropped.swap(0, Ordering::Relaxed);
                if dropped > 0 {
                    let note = Line::new(
                        now(),
                        Level::WARN,
                        LOADER_ID,
                        &format!("{dropped} log lines were dropped: they came faster than the disk took them"),
                    );
                    all.write(note.text.as_bytes());
                    keep(note);
                }
                all.write(line.text.as_bytes());
                let source = line.source();
                if !files.contains_key(source) {
                    let path = if source == LOADER_ID {
                        dir.join("loader.log")
                    } else {
                        mods.join(format!("{source}.log"))
                    };
                    files.insert(source.to_owned(), Capped::create(path, SOURCE_CAP).ok());
                }
                if let Some(Some(file)) = files.get_mut(source) {
                    file.write(line.text.as_bytes());
                }
                keep(line);
                COUNTS.taken.fetch_add(1, Ordering::Release);
            }
        });
    match started {
        Ok(_) => {
            let _first = LINES.set(send);
        }
        Err(e) => crate::early::note(&format!("no log: its thread did not start: {e}")),
    }
}

/// A log file of the writer that moves aside whole as its latest earlier run
/// once it would pass its cap, so a server running for weeks never fills its
/// disk.
struct Capped {
    path: PathBuf,
    /// `None` while the file could not be opened again; each write tries.
    file: Option<std::fs::File>,
    written: u64,
    cap: u64,
}

impl Capped {
    /// Makes the file at `path` anew, empty.
    fn create(path: PathBuf, cap: u64) -> std::io::Result<Self> {
        let file = std::fs::File::create(&path)?;
        Ok(Self {
            path,
            file: Some(file),
            written: 0,
            cap,
        })
    }

    /// Appends `bytes` and flushes them, rolling the file over first when
    /// they would take it past its cap.
    fn write(&mut self, bytes: &[u8]) {
        let len = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        if self.written > 0 && self.written.saturating_add(len) > self.cap {
            self.roll();
        }
        if self.file.is_none() {
            self.file = Self::append(&self.path);
        }
        // Flush per line: after a crash the last lines are the useful ones.
        if let Some(file) = &mut self.file
            && file.write_all(bytes).and_then(|()| file.flush()).is_ok()
        {
            self.written = self.written.saturating_add(len);
        }
    }

    /// Moves the file aside whole as run 1 and starts it again with a line
    /// saying so. When it cannot move, the file grows on and the next roll
    /// waits for another cap's worth of lines.
    #[cold]
    fn roll(&mut self) {
        // Closed before the rename, which Windows needs.
        self.file = None;
        self.written = 0;
        let moved =
            make_room(&self.path).is_some_and(|first| std::fs::rename(&self.path, first).is_ok());
        if !moved {
            return;
        }
        self.file = Self::append(&self.path);
        if !enabled(Level::INFO, LOADER_ID) {
            return;
        }
        let name = self
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned());
        let note = Line::new(
            now(),
            Level::INFO,
            LOADER_ID,
            &format!(
                "{} reached its cap; the lines of this run before this one are in the file one run older",
                name.unwrap_or_default()
            ),
        );
        if let Some(file) = &mut self.file
            && file.write_all(note.text.as_bytes()).is_ok()
        {
            self.written = u64::try_from(note.text.len()).unwrap_or(u64::MAX);
        }
    }

    /// The file at `path` opened to append to, made when missing.
    fn append(path: &Path) -> Option<std::fs::File> {
        std::fs::File::options()
            .create(true)
            .append(true)
            .open(path)
            .ok()
    }
}

/// Moves every current `<name>.log` in `dir` aside as `<name>.1.log`, the
/// one before to `<name>.2.log`, dropping the oldest.
fn rotate_dir(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let current: Vec<PathBuf> = entries
        .filter_map(|entry| Some(entry.ok()?.path()))
        .filter(|path| path.is_file() && run_of(path) == Some(0))
        .collect();
    for path in current {
        rotate(&path);
    }
}

/// The run a log file of the writer belongs to: 0 for `<name>.log`, `n` for
/// `<name>.<n>.log`; `None` for any other file.
fn run_of(path: &Path) -> Option<usize> {
    if path.extension()? != "log" {
        return None;
    }
    let stem = path.file_stem()?.to_str()?;
    match stem.rsplit_once('.') {
        Some((_, run)) => run.parse().ok().filter(|&run| run > 0),
        None => Some(0),
    }
}

/// `<name>.<run>.log` beside `current`, `<name>.log`.
fn run_path(current: &Path, run: usize) -> Option<PathBuf> {
    let stem = current.file_stem()?.to_str()?;
    Some(current.with_file_name(format!("{stem}.{run}.log")))
}

/// Moves `current` aside as run 1, run 1 to run 2 and so on up to
/// [`KEPT_RUNS`], at start; a file over [`KEPT_BYTES`] keeps its last lines
/// that fit, or all of them when those cannot be written.
fn rotate(current: &Path) {
    if !current.is_file() {
        return;
    }
    let Some(first) = make_room(current) else {
        return;
    };
    let size = std::fs::metadata(current).map_or(0, |m| m.len());
    if size > KEPT_BYTES && keep_tail(current, &first, KEPT_BYTES).is_ok() {
        let _removed = std::fs::remove_file(current);
    } else {
        let _moved = std::fs::rename(current, &first);
    }
}

/// Moves run 1 of `current` to run 2 and so on up to [`KEPT_RUNS`], dropping
/// the oldest; run 1's path, now free.
fn make_room(current: &Path) -> Option<PathBuf> {
    let runs = (1..=KEPT_RUNS)
        .map(|run| run_path(current, run))
        .collect::<Option<Vec<_>>>()?;
    let _oldest = runs.last().map(std::fs::remove_file);
    for pair in runs.windows(2).rev() {
        let _moved = std::fs::rename(&pair[0], &pair[1]);
    }
    runs.into_iter().next()
}

/// Writes the last whole lines of `from` that fit in `bytes` to `to`.
fn keep_tail(from: &Path, to: &Path, bytes: u64) -> std::io::Result<()> {
    use std::io::{Read as _, Seek as _, SeekFrom};
    let mut file = std::fs::File::open(from)?;
    let size = file.metadata()?.len();
    file.seek(SeekFrom::Start(size.saturating_sub(bytes)))?;
    let mut tail = Vec::new();
    file.read_to_end(&mut tail)?;
    let start = if size > bytes {
        tail.iter()
            .position(|&b| b == b'\n')
            .map_or(tail.len(), |at| at + 1)
    } else {
        0
    };
    std::fs::write(to, &tail[start..])
}

/// Whether lines go to [`CAPTURED`] instead of the files.
static CAPTURING: AtomicBool = AtomicBool::new(false);
/// Lines kept in memory instead of written, for a dry run. Its lock stays
/// off the loader's poison policy: recovering it would log, and logging takes
/// this lock, so a poisoned capture drops lines instead.
static CAPTURED: Mutex<Option<Vec<(Level, String, String)>>> = Mutex::new(None);

/// Keeps every line from now on in memory, for [`captured`].
pub fn capture() {
    // Off the poison policy, as `CAPTURED` says.
    // ast-grep-ignore: lock-through-relock
    if let Ok(mut lines) = CAPTURED.lock() {
        *lines = Some(Vec::new());
    }
    CAPTURING.store(true, Ordering::Release);
}

/// The lines kept since [`capture`]: level, source and message.
#[must_use]
pub fn captured() -> Vec<(Level, String, String)> {
    // Off the poison policy, as `CAPTURED` says.
    // ast-grep-ignore: lock-through-relock
    CAPTURED
        .lock()
        .ok()
        .and_then(|lines| lines.clone())
        .unwrap_or_default()
}

/// Stops [`capture`], so the tests sharing the process log as before.
#[cfg(all(test, windows))]
pub(crate) fn end_capture() {
    CAPTURING.store(false, Ordering::Release);
    if let Ok(mut lines) = CAPTURED.lock() {
        *lines = None;
    }
}

/// Writes one line under `source`, the loader or a mod id.
pub fn write(level: Level, source: &str, message: &str) {
    if CAPTURING.load(Ordering::Acquire) {
        // Off the poison policy, as `CAPTURED` says.
        // ast-grep-ignore: lock-through-relock
        if let Ok(mut lines) = CAPTURED.lock()
            && let Some(lines) = lines.as_mut()
        {
            lines.push((level, source.to_owned(), message.to_owned()));
        }
        return;
    }
    let Some(lines) = LINES.get() else {
        return;
    };
    if !enabled(level, source) {
        return;
    }
    match lines.try_send(Line::new(now(), level, source, message)) {
        Ok(()) => {
            COUNTS.sent.fetch_add(1, Ordering::Relaxed);
        }
        Err(TrySendError::Full(_)) => {
            COUNTS.dropped.fetch_add(1, Ordering::Relaxed);
        }
        Err(TrySendError::Disconnected(_)) => {}
    }
}

/// Waits until the writer keeps every line sent before this call among the
/// latest, so [`recent`] holds them, or `longest` passes; whether it did.
/// Never on a game thread: it sleeps.
pub fn settle(longest: Duration) -> bool {
    let sent = COUNTS.sent.load(Ordering::Relaxed);
    let started = Instant::now();
    while COUNTS.taken.load(Ordering::Acquire) < sent {
        if started.elapsed() >= longest {
            return false;
        }
        std::thread::sleep(SETTLE_POLL);
    }
    true
}

/// Keeps `line` among the latest lines.
fn keep(line: Line) {
    RECENT_LINES.relock().keep(line);
}

/// The latest [`RECENT`] lines, oldest first.
#[derive(Debug)]
struct Recent {
    lines: VecDeque<Line>,
}

impl Recent {
    const fn new() -> Self {
        Self {
            lines: VecDeque::new(),
        }
    }

    fn keep(&mut self, line: Line) {
        if self.lines.len() == RECENT {
            self.lines.pop_front();
        }
        self.lines.push_back(line);
    }

    fn latest(&self, source: Option<&str>, level: Level, count: usize) -> Vec<String> {
        let mut found: Vec<String> = self
            .lines
            .iter()
            .rev()
            .filter(|line| {
                line.level <= level && source.is_none_or(|source| source == line.source())
            })
            .take(count)
            .map(|line| line.text.trim_end().to_owned())
            .collect();
        found.reverse();
        found
    }
}

/// The latest `count` kept lines from `source` (every source for `None`) at
/// `level` or more severe, oldest first, without their line ends.
#[must_use]
pub fn recent(source: Option<&str>, level: Level, count: usize) -> Vec<String> {
    RECENT_LINES.relock().latest(source, level, count)
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

/// Logs a line from the loader itself; formats it only when its level is
/// kept.
#[macro_export]
macro_rules! log {
    ($level:ident, $($arg:tt)+) => {{
        let level = ::enshwick::abi::Level::$level;
        if $crate::log::enabled(level, $crate::settings::LOADER_ID) {
            $crate::log::write(level, $crate::settings::LOADER_ID, &::std::format!($($arg)+));
        }
    }};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_latest_lines_are_kept_bounded_and_filtered() {
        let mut ring = Recent::new();
        for i in 0..RECENT + 10 {
            let level = if i % 2 == 0 { Level::INFO } else { Level::WARN };
            let source = if i % 3 == 0 { "a" } else { "b" };
            ring.keep(Line::new(0, level, source, &format!("line {i}")));
        }
        assert_eq!(ring.lines.len(), RECENT);
        let message = |line: &String| line.rsplit_once("] ").unwrap().1.to_owned();
        let last = RECENT + 9;
        assert_eq!(
            ring.latest(None, Level::DEBUG, 2)
                .iter()
                .map(message)
                .collect::<Vec<_>>(),
            [format!("line {}", last - 1), format!("line {last}")]
        );
        let warnings = ring.latest(Some("b"), Level::WARN, 3);
        assert_eq!(warnings.len(), 3);
        assert!(warnings.iter().map(message).all(|line| {
            let i: usize = line.trim_start_matches("line ").parse().unwrap();
            i % 2 == 1 && !i.is_multiple_of(3)
        }));
        assert!(ring.latest(Some("nobody"), Level::DEBUG, 5).is_empty());
    }

    #[test]
    fn a_line_written_before_settling_is_among_the_latest() {
        let dir = std::env::temp_dir().join(format!("enshwick-settle-{}", std::process::id()));
        open(&dir);
        write(Level::ERROR, LOADER_ID, "the line a report needs");
        assert!(settle(Duration::from_secs(10)));
        let latest = recent(Some(LOADER_ID), Level::ERROR, RECENT);
        assert!(
            latest
                .iter()
                .any(|line| line.ends_with("the line a report needs")),
            "{latest:?}"
        );
        let _removed = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_line_below_its_source_s_level_is_not_formatted() {
        set_levels(Settings {
            log_level: Level::WARN,
            log_levels: [("chatty".to_owned(), Level::DEBUG)].into(),
            ..Settings::default()
        });
        assert!(enabled(Level::ERROR, LOADER_ID));
        assert!(!enabled(Level::INFO, LOADER_ID));
        assert!(enabled(Level::DEBUG, "chatty"));
        let formatted = core::cell::Cell::new(false);
        let costly = || {
            formatted.set(true);
            "costly"
        };
        crate::log!(DEBUG, "{}", costly());
        assert!(!formatted.get());
        crate::log!(ERROR, "{}", costly());
        assert!(formatted.get());
    }

    #[test]
    fn earlier_runs_are_kept_twice_and_capped() {
        let dir = std::env::temp_dir().join(format!("enshwick-log-{}", std::process::id()));
        let _fresh = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let read = |name: &str| std::fs::read_to_string(dir.join(name)).ok();
        for run in ["first", "second", "third"] {
            std::fs::write(dir.join("all.log"), run).unwrap();
            std::fs::write(dir.join("notes.txt"), run).unwrap();
            rotate_dir(&dir);
        }
        assert_eq!(read("all.log"), None);
        assert_eq!(read("all.1.log").as_deref(), Some("third"));
        assert_eq!(read("all.2.log").as_deref(), Some("second"));
        assert_eq!(read("all.3.log"), None);
        assert_eq!(read("notes.txt").as_deref(), Some("third"));
        assert_eq!(read("notes.1.txt"), None);

        let big = dir.join("big.log");
        std::fs::write(&big, "aaaa\nbbbb\ncccc\n").unwrap();
        keep_tail(&big, &dir.join("tail.log"), 7).unwrap();
        assert_eq!(read("tail.log").as_deref(), Some("cccc\n"));
        keep_tail(&big, &dir.join("tail.log"), 100).unwrap();
        assert_eq!(read("tail.log").as_deref(), Some("aaaa\nbbbb\ncccc\n"));
        let _clean = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_file_past_its_cap_rolls_over_whole_and_loses_no_line() {
        let dir = std::env::temp_dir().join(format!("enshwick-log-cap-{}", std::process::id()));
        let _fresh = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("all.log");
        let mut file = Capped::create(path.clone(), 400).unwrap();
        for i in 0..40 {
            file.write(format!("line {i:02} of the run\n").as_bytes());
        }
        drop(file);
        let read = |name: &str| std::fs::read_to_string(dir.join(name)).unwrap();
        let numbers = |text: &str| -> Vec<u32> {
            text.lines()
                .filter_map(|line| line.strip_prefix("line ")?.get(..2)?.parse().ok())
                .collect()
        };
        let (oldest, earlier, current) = (read("all.2.log"), read("all.1.log"), read("all.log"));
        for text in [&earlier, &current] {
            assert!(text.len() <= 400, "{text}");
            assert!(
                text.lines().next().is_some_and(|note| note.ends_with(
                    "[enshwick] all.log reached its cap; the lines of this run before this one are in the file one run older"
                )),
                "{text}"
            );
        }
        let kept: Vec<u32> = [&oldest, &earlier, &current]
            .into_iter()
            .flat_map(|text| numbers(text))
            .collect();
        let last = *kept.last().unwrap();
        assert_eq!(last, 39);
        assert_eq!(
            kept,
            (last + 1 - u32::try_from(kept.len()).unwrap()..=last).collect::<Vec<_>>(),
            "the kept runs hold every line from the oldest kept one on"
        );
        assert!(!dir.join("all.3.log").exists());
        let _clean = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn only_the_writer_s_logs_have_runs() {
        assert_eq!(run_of(Path::new("all.log")), Some(0));
        assert_eq!(run_of(Path::new("all.2.log")), Some(2));
        assert_eq!(run_of(Path::new("all.0.log")), None);
        assert_eq!(run_of(Path::new("all.txt")), None);
        assert_eq!(run_of(Path::new("all.x.log")), None);
    }

    #[test]
    fn a_line_names_its_time_level_and_source() {
        let line = Line::new(3_723_004, Level::WARN, "you.lockers", "opened [3] lockers");
        assert_eq!(
            line.text,
            "01:02:03.004Z warn  [you.lockers] opened [3] lockers\n"
        );
        assert_eq!(line.source(), "you.lockers");
        assert_eq!(line.level, Level::WARN);
    }
}
