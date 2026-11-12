//! `logs/<source>_<YYYY-MM-DD_HH-MM-SS>.log`: one file per source, the
//! loader (`enshwick`) or a mod id, per run of the process.
//!
//! Callers only format a line and send it down a bounded channel; a
//! background thread owns the files, so a hook never waits on the disk. A
//! line the full channel cannot take is counted and the count logged once
//! there is room. The writer also keeps the latest lines in memory for
//! `enshwick log` on the console. Each source keeps its newest [`KEPT_RUNS`]
//! files; a file that would pass [`CAP`] during a run is closed and the
//! source goes on in a new file stamped with the time it opens, so a server
//! running for weeks never fills its disk. Deleting any file by hand is safe.

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
use enshwick_core::abi::Level;

use crate::engine::settings::{LOADER_ID, Settings};
use enshwick_base::Relock;

/// The folder beside the game's executable that holds every log.
pub const LOGS: &str = "logs";
/// The file in [`LOGS`] that keeps the lines written before the log opens.
///
/// The stub's lines are among them; the log takes them into the loader's
/// file and removes it, so it stays only when the loader did not start.
pub const START_FILE: &str = "start.log";
/// Lines waiting for the writer at most.
const QUEUE: usize = 4096;
/// The latest lines kept in memory.
const RECENT: usize = 1024;
/// How often [`settle`] looks at the writer.
const SETTLE_POLL: Duration = Duration::from_millis(2);
/// Files kept per source, the current one among them.
const KEPT_RUNS: usize = 10;
/// The most bytes one file holds.
const CAP: u64 = 8 << 20;
/// Seconds ahead a file that reached its cap looks for a free stamp to go
/// on under; past them it grows on.
const ROLL_SECONDS: u64 = 60;
/// The loader's file of this run, once the log opens.
static LOADER_FILE: OnceLock<PathBuf> = OnceLock::new();

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

/// The loader's file of this run, once the log opens; a crash writes its
/// line there too.
#[must_use]
pub fn loader_file() -> Option<&'static Path> {
    LOADER_FILE.get().map(PathBuf::as_path)
}

/// Starts the writer in `dir`, the game's `logs/`.
///
/// Each source's oldest files beyond [`KEPT_RUNS`] are deleted as it opens
/// its file, and the lines of [`START_FILE`] open the loader's file. When
/// it cannot start, [`crate::engine::early`] says why.
pub fn open(dir: &Path) {
    enshwick_base::__internal::report_poison_to(poisoned_at);
    if let Err(e) = std::fs::create_dir_all(dir) {
        crate::engine::early::note(&format!("no log: {} could not be made: {e}", dir.display()));
        return;
    }
    let dir = dir.to_path_buf();
    let stamp = crate::engine::early::log_stamp(now() / 1000);
    let _first = LOADER_FILE.set(run_path(&dir, LOADER_ID, &stamp));
    let (send, receive) = sync_channel::<Line>(QUEUE);
    let started = std::thread::Builder::new()
        .name("enshwick-log".into())
        .spawn(move || {
            // On the writer's thread: the game's start never waits on the
            // disk, and lines sent meanwhile wait in the channel.
            let mut files: rustc_hash::FxHashMap<String, Option<Capped>> =
                rustc_hash::FxHashMap::default();
            let mut loader = Capped::open(&dir, LOADER_ID, &stamp);
            if let Some(file) = &mut loader {
                take_start_lines(&dir, file);
            }
            files.insert(LOADER_ID.to_owned(), loader);
            for line in receive {
                let dropped = COUNTS.dropped.swap(0, Ordering::Relaxed);
                if dropped > 0 {
                    let note = Line::new(
                        now(),
                        Level::WARN,
                        LOADER_ID,
                        &format!("{dropped} log lines were dropped: they came faster than the disk took them"),
                    );
                    if let Some(Some(file)) = files.get_mut(LOADER_ID) {
                        file.write(note.text.as_bytes());
                    }
                    keep(note);
                }
                let source = line.source();
                if !files.contains_key(source) {
                    files.insert(source.to_owned(), Capped::open(&dir, source, &stamp));
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
        Err(e) => crate::engine::early::note(&format!("no log: its thread did not start: {e}")),
    }
}

/// Writes to the loader's log that the lock taken at `at` was poisoned, for
/// the lock policy of `enshwick-base`, which reports there once the log
/// opens.
#[cold]
fn poisoned_at(at: &std::panic::Location<'_>) {
    crate::log!(
        WARN,
        "{}:{}: a thread panicked holding this lock; its state is kept",
        at.file(),
        at.line()
    );
}

/// Moves the lines of [`START_FILE`] in `dir` to the top of `file`.
fn take_start_lines(dir: &Path, file: &mut Capped) {
    let path = dir.join(START_FILE);
    if let Ok(lines) = std::fs::read(&path) {
        file.write(&lines);
        let _removed = std::fs::remove_file(&path);
    }
}

/// `<source>_<stamp>.log` in `dir`.
fn run_path(dir: &Path, source: &str, stamp: &str) -> PathBuf {
    dir.join(format!("{source}_{stamp}.log"))
}

/// The length of a file's stamp, `2026-11-03_10-00-00`.
const STAMP_LEN: usize = 19;

/// The source a file of the writer belongs to: `you.my-mod` for
/// `you.my-mod_2026-11-03_10-00-00.log`; `None` for any other file.
fn source_of(name: &str) -> Option<&str> {
    let base = name.strip_suffix(".log")?;
    let (source, stamp) = base.split_at_checked(base.len().checked_sub(STAMP_LEN)?)?;
    let source = source.strip_suffix('_')?;
    let shaped = stamp.bytes().enumerate().all(|(i, b)| match i {
        4 | 7 | 13 | 16 => b == b'-',
        10 => b == b'_',
        _ => b.is_ascii_digit(),
    });
    (shaped && !source.is_empty()).then_some(source)
}

/// Deletes the oldest files of `source` in `dir` so that `keep` remain;
/// the stamps sort by time.
pub(crate) fn prune(dir: &Path, source: &str, keep: usize) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut runs: Vec<PathBuf> = entries
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            let name = path.file_name()?.to_str()?;
            (source_of(name) == Some(source)).then(|| path.clone())
        })
        .collect();
    runs.sort();
    let extra = runs.len().saturating_sub(keep);
    for path in &runs[..extra] {
        let _removed = std::fs::remove_file(path);
    }
}

/// A source's file of this run, which goes on in a new file once it would
/// pass [`CAP`].
struct Capped {
    dir: PathBuf,
    source: String,
    path: PathBuf,
    /// `None` while the file could not be opened again; each write tries.
    file: Option<std::fs::File>,
    written: u64,
}

impl Capped {
    /// Makes `source`'s file of the run stamped `stamp` in `dir`, deleting
    /// its oldest files first; `None` when it cannot be made.
    fn open(dir: &Path, source: &str, stamp: &str) -> Option<Self> {
        prune(dir, source, KEPT_RUNS - 1);
        let path = run_path(dir, source, stamp);
        let file = Self::append(&path)?;
        let written = file.metadata().map_or(0, |m| m.len());
        Some(Self {
            dir: dir.to_path_buf(),
            source: source.to_owned(),
            path,
            file: Some(file),
            written,
        })
    }

    /// Appends `bytes` and flushes them, going on in a new file first when
    /// they would take this one past its cap.
    fn write(&mut self, bytes: &[u8]) {
        let len = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        if self.written > 0 && self.written.saturating_add(len) > CAP {
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

    /// Ends the file with a line naming the next one and goes on there.
    #[cold]
    fn roll(&mut self) {
        let next = (0..ROLL_SECONDS)
            .map(|wait| crate::engine::early::log_stamp(now() / 1000 + wait))
            .map(|stamp| run_path(&self.dir, &self.source, &stamp))
            .find(|path| *path != self.path && !path.exists());
        let Some(next) = next else {
            return;
        };
        let name = next
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let note = Line::new(
            now(),
            Level::INFO,
            LOADER_ID,
            &format!("this file reached its cap; the run goes on in {name}"),
        );
        if let Some(file) = &mut self.file {
            let _noted = file.write_all(note.text.as_bytes());
        }
        prune(&self.dir, &self.source, KEPT_RUNS - 1);
        self.path = next;
        self.file = Self::append(&self.path);
        self.written = 0;
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
        let level = ::enshwick_core::abi::Level::$level;
        if $crate::engine::log::enabled(level, $crate::engine::settings::LOADER_ID) {
            $crate::engine::log::write(level, $crate::engine::settings::LOADER_ID, &::std::format!($($arg)+));
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

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("enshwick-{name}-{}", std::process::id()));
        let _fresh = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn a_file_names_its_source_and_its_run() {
        assert_eq!(
            source_of("you.my-mod_2026-11-03_10-00-00.log"),
            Some("you.my-mod")
        );
        assert_eq!(
            source_of("enshwick_2026-11-03_10-00-00.log"),
            Some("enshwick")
        );
        assert_eq!(
            source_of("enshwick-crash_2026-11-03_10-00-00.log"),
            Some("enshwick-crash")
        );
        assert_eq!(source_of("start.log"), None);
        assert_eq!(source_of("enshwick_2026-11-03.log"), None);
        assert_eq!(source_of("_2026-11-03_10-00-00.log"), None);
        assert_eq!(source_of("enshwick_2026-11-03_10-00-00.txt"), None);
        assert_eq!(source_of("enshwick_2026x11-03_10-00-00.log"), None);
        assert_eq!(
            run_path(
                Path::new("logs"),
                "enshwick",
                &crate::engine::early::log_stamp(1_793_286_189)
            ),
            Path::new("logs").join("enshwick_2026-10-29_15-03-09.log")
        );
    }

    #[test]
    fn each_source_keeps_its_newest_runs_and_other_files_stay() {
        let dir = temp("log-prune");
        for day in 10..25 {
            for source in ["enshwick", "you.my-mod"] {
                let name = format!("{source}_2026-11-{day}_10-00-00.log");
                std::fs::write(dir.join(name), "").unwrap();
            }
        }
        std::fs::write(dir.join("notes.txt"), "").unwrap();
        drop(Capped::open(&dir, "enshwick", "2026-12-01_00-00-00").unwrap());
        let kept = names(&dir);
        let ours: Vec<&String> = kept.iter().filter(|n| n.starts_with("enshwick_")).collect();
        assert_eq!(ours.len(), KEPT_RUNS);
        assert_eq!(ours[0], "enshwick_2026-11-16_10-00-00.log");
        assert_eq!(ours[KEPT_RUNS - 1], "enshwick_2026-12-01_00-00-00.log");
        assert_eq!(kept.iter().filter(|n| n.starts_with("you.")).count(), 15);
        assert!(kept.contains(&"notes.txt".to_owned()));
        let _clean = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_file_past_its_cap_goes_on_in_a_new_one_and_loses_no_line() {
        let dir = temp("log-cap");
        let mut file = Capped::open(&dir, "you.my-mod", "2020-01-01_10-00-00").unwrap();
        file.written = CAP - 10;
        file.write(b"the first line\n");
        file.write(b"the second line\n");
        drop(file);
        let kept = names(&dir);
        assert_eq!(kept.len(), 2, "{kept:?}");
        assert_eq!(kept[0], "you.my-mod_2020-01-01_10-00-00.log");
        let first = std::fs::read_to_string(dir.join(&kept[0])).unwrap();
        assert!(
            first
                .trim_end()
                .ends_with(&format!("the run goes on in {}", kept[1])),
            "{first}"
        );
        let next = std::fs::read_to_string(dir.join(&kept[1])).unwrap();
        assert_eq!(next, "the first line\nthe second line\n");
        let _clean = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_start_lines_open_the_loader_s_file_and_their_file_goes() {
        let dir = temp("log-start");
        std::fs::write(dir.join(START_FILE), "stub: early\n").unwrap();
        let mut file = Capped::open(&dir, LOADER_ID, "2026-11-03_10-00-00").unwrap();
        take_start_lines(&dir, &mut file);
        file.write(b"later\n");
        drop(file);
        assert!(!dir.join(START_FILE).exists());
        assert_eq!(
            std::fs::read_to_string(dir.join("enshwick_2026-11-03_10-00-00.log")).unwrap(),
            "stub: early\nlater\n"
        );
        let _clean = std::fs::remove_dir_all(&dir);
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
