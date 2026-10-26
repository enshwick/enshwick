//! `logs/enshwick/` (`all.log`, `loader.log`, `mods/<id>.log`): one file for the loader and every mod.
//!
//! Callers only format a line and send it down a bounded channel; a
//! background thread owns the files, so a hook never waits on the disk. A
//! line the full channel cannot take is counted and the count logged once
//! there is room. The writer also keeps the latest lines in memory for
//! `enshwick log` on the console. The writer keeps the two runs before as `<name>.1.log`
//! and `<name>.2.log`, each at most [`KEPT_BYTES`].

use std::collections::VecDeque;
use std::fmt::Write as _;
use std::io::Write as _;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{SyncSender, TrySendError, sync_channel};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use arc_swap::ArcSwapOption;
use enshwick::abi::Level;

use crate::locks::Relock;
use crate::settings::{LOADER_ID, Settings};

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

/// One line: its level, its source and the line as written.
type Line = (Level, String, String);

static LINES: OnceLock<SyncSender<Line>> = OnceLock::new();
/// What the channel to the writer has carried.
struct Counts {
    /// Lines the full channel could not take since the writer last said so.
    dropped: AtomicU64,
    /// Lines sent to the writer, for [`settle`].
    sent: AtomicU64,
    /// Lines the writer has kept among the latest, for [`settle`].
    taken: AtomicU64,
}

static COUNTS: Counts = Counts {
    dropped: AtomicU64::new(0),
    sent: AtomicU64::new(0),
    taken: AtomicU64::new(0),
};
/// The latest [`RECENT`] lines, oldest first.
static RECENT_LINES: Mutex<VecDeque<Line>> = Mutex::new(VecDeque::new());
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

/// Starts the writer in `dir`, the previous runs' files moved aside first:
/// `all.log` gets every line, `loader.log` the loader's, `mods/<id>.log` each
/// mod's.
pub fn open(dir: &Path) {
    let mods = dir.join("mods");
    if std::fs::create_dir_all(&mods).is_err() {
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
            let Ok(mut all) = std::fs::File::create(dir.join("all.log")) else {
                return;
            };
            let mut files: rustc_hash::FxHashMap<String, Option<std::fs::File>> =
                rustc_hash::FxHashMap::default();
            for (level, source, line) in receive {
                let dropped = COUNTS.dropped.swap(0, Ordering::Relaxed);
                if dropped > 0 {
                    let note = format_line(
                        now(),
                        Level::WARN,
                        LOADER_ID,
                        &format!("{dropped} log lines were dropped: they came faster than the disk took them"),
                    );
                    let _all = all.write_all(note.as_bytes());
                    keep((Level::WARN, LOADER_ID.to_owned(), note));
                }
                keep((level, source.clone(), line.clone()));
                COUNTS.taken.fetch_add(1, Ordering::Release);
                let _all = all.write_all(line.as_bytes()).and_then(|()| all.flush());
                let file = files.entry(source).or_insert_with_key(|source| {
                    let path = if source == LOADER_ID {
                        dir.join("loader.log")
                    } else {
                        mods.join(format!("{source}.log"))
                    };
                    std::fs::File::create(path).ok()
                });
                // Flush per line: after a crash the last lines are the useful ones.
                if let Some(file) = file {
                    let _own = file.write_all(line.as_bytes()).and_then(|()| file.flush());
                }
            }
        });
    if started.is_ok() {
        let _first = LINES.set(send);
    }
}

/// Moves every current `<name>.log` in `dir` aside as `<name>.1.log`, the
/// one before to `<name>.2.log`, dropping the oldest.
fn rotate_dir(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let current: Vec<std::path::PathBuf> = entries
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
fn run_path(current: &Path, run: usize) -> Option<std::path::PathBuf> {
    let stem = current.file_stem()?.to_str()?;
    Some(current.with_file_name(format!("{stem}.{run}.log")))
}

/// Moves `current` aside as run 1, run 1 to run 2 and so on up to
/// [`KEPT_RUNS`]; a file over [`KEPT_BYTES`] keeps its last lines that fit.
fn rotate(current: &Path) {
    if !current.is_file() {
        return;
    }
    let Some(runs) = (1..=KEPT_RUNS)
        .map(|run| run_path(current, run))
        .collect::<Option<Vec<_>>>()
    else {
        return;
    };
    let _oldest = runs.last().map(std::fs::remove_file);
    for pair in runs.windows(2).rev() {
        let _moved = std::fs::rename(&pair[0], &pair[1]);
    }
    let size = std::fs::metadata(current).map_or(0, |m| m.len());
    if size <= KEPT_BYTES {
        let _moved = std::fs::rename(current, &runs[0]);
    } else {
        let _kept = keep_tail(current, &runs[0], KEPT_BYTES);
        let _removed = std::fs::remove_file(current);
    }
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
static CAPTURING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// Lines kept in memory instead of written, for a dry run. Its lock stays
/// off the loader's poison policy: recovering it would log, and logging takes
/// this lock, so a poisoned capture drops lines instead.
static CAPTURED: Mutex<Option<Vec<(Level, String, String)>>> = Mutex::new(None);

/// Keeps every line from now on in memory, for [`captured`].
pub fn capture() {
    if let Ok(mut lines) = CAPTURED.lock() {
        *lines = Some(Vec::new());
    }
    CAPTURING.store(true, Ordering::Release);
}

/// The lines kept since [`capture`]: level, source and message.
#[must_use]
pub fn captured() -> Vec<(Level, String, String)> {
    CAPTURED
        .lock()
        .ok()
        .and_then(|lines| lines.clone())
        .unwrap_or_default()
}

/// Writes one line under `source`, the loader or a mod id.
pub fn write(level: Level, source: &str, message: &str) {
    if CAPTURING.load(Ordering::Acquire) {
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
    let line = (
        level,
        source.to_owned(),
        format_line(now(), level, source, message),
    );
    match lines.try_send(line) {
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
    let mut recent = RECENT_LINES.relock();
    if recent.len() == RECENT {
        recent.pop_front();
    }
    recent.push_back(line);
}

/// The latest `count` kept lines from `source` (every source for `None`) at
/// `level` or more severe, oldest first, without their line ends.
#[must_use]
pub fn recent(source: Option<&str>, level: Level, count: usize) -> Vec<String> {
    let recent = RECENT_LINES.relock();
    let mut found: Vec<String> = recent
        .iter()
        .rev()
        .filter(|(l, s, _)| *l <= level && source.is_none_or(|source| source == s))
        .take(count)
        .map(|(_, _, line)| line.trim_end().to_owned())
        .collect();
    found.reverse();
    found
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

fn format_line(millis: u64, level: Level, source: &str, message: &str) -> String {
    let secs = millis / 1000;
    let (h, m, s) = (secs / 3600 % 24, secs / 60 % 60, secs % 60);
    let level = level.name();
    let mut line = String::with_capacity(32 + source.len() + message.len());
    let _formatted = writeln!(
        line,
        "{h:02}:{m:02}:{s:02}.{:03}Z {level:<5} [{source}] {message}",
        millis % 1000
    );
    line
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
        for i in 0..RECENT + 10 {
            let level = if i % 2 == 0 { Level::INFO } else { Level::WARN };
            let source = if i % 3 == 0 { "a" } else { "b" };
            keep((level, source.to_owned(), format!("line {i}\n")));
        }
        assert_eq!(RECENT_LINES.relock().len(), RECENT);
        let last = RECENT + 9;
        assert_eq!(
            recent(None, Level::DEBUG, 2),
            [format!("line {}", last - 1), format!("line {last}")]
        );
        let warnings = recent(Some("b"), Level::WARN, 3);
        assert_eq!(warnings.len(), 3);
        assert!(warnings.iter().all(|line| {
            let i: usize = line.trim_start_matches("line ").parse().unwrap();
            i % 2 == 1 && !i.is_multiple_of(3)
        }));
        assert!(recent(Some("nobody"), Level::DEBUG, 5).is_empty());
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
    fn only_the_writer_s_logs_have_runs() {
        assert_eq!(run_of(Path::new("all.log")), Some(0));
        assert_eq!(run_of(Path::new("all.2.log")), Some(2));
        assert_eq!(run_of(Path::new("all.0.log")), None);
        assert_eq!(run_of(Path::new("all.txt")), None);
        assert_eq!(run_of(Path::new("all.x.log")), None);
    }

    #[test]
    fn a_line_names_its_time_level_and_source() {
        let line = format_line(3_723_004, Level::WARN, "you.lockers", "opened 3 lockers");
        assert_eq!(line, "01:02:03.004Z warn  [you.lockers] opened 3 lockers\n");
    }
}
