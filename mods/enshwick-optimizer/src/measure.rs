//! Measure mode: once a minute, a line with the CPU the process, its main
//! thread, the server loop's thread and its busiest groups of threads used,
//! the server loop's passes and steps, the exact waits slept and the server's
//! "Bad Performance" warnings.
//!
//! The warnings come from the game's own log beside the game,
//! `logs/<game>.log`, read from where the last minute stopped.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::io::{Read as _, Seek as _, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use crate::sys::{self, Thread};
use crate::waits::{self, CAPPED, LATE_NS, PASSES, SERVER_THREAD, SLEEPS, SLEPT_NS, STEPS};

/// How often a line is written.
pub(crate) const PERIOD: Duration = Duration::from_mins(1);
/// The game's warning for a slow server pass.
const BAD_PERFORMANCE: &str = "[server] Bad Performance ";
/// The most of the game log read in one minute.
const MOST_READ: u64 = 4 << 20;
/// How many groups of threads a line names.
const THREAD_GROUPS: usize = 6;

/// Whether the lines are written.
static ON: AtomicBool = AtomicBool::new(false);
/// The last line, for the status summary.
static LAST: Mutex<String> = Mutex::new(String::new());
/// The platform and what runs on it, ahead of each line.
static CONTEXT: Mutex<String> = Mutex::new(String::new());

/// Turns measure mode on or off; `context` names the platform and what runs.
pub(crate) fn set(on: bool, context: String) {
    ON.store(on, Ordering::Relaxed);
    if !on {
        LAST.lock().unwrap_or_else(PoisonError::into_inner).clear();
    }
    *CONTEXT.lock().unwrap_or_else(PoisonError::into_inner) = context;
}

/// The platform and what runs on it.
pub(crate) fn context() -> String {
    CONTEXT
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

/// The last line written, empty before the first.
pub(crate) fn last() -> String {
    LAST.lock().unwrap_or_else(PoisonError::into_inner).clone()
}

/// The "Bad Performance" warnings in a piece of the game log: how many, and
/// the longest pass they name, in µs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Warnings {
    /// How many warnings.
    pub(crate) count: u64,
    /// The longest pass named, in µs.
    pub(crate) worst_us: u64,
}

/// Counts the warnings in `text`, lines such as `[server] Bad Performance
/// 24,700 us (peak: 34,012 us)`.
#[must_use]
pub(crate) fn warnings(text: &str) -> Warnings {
    let mut found = Warnings::default();
    for rest in text
        .lines()
        .filter_map(|line| line.split(BAD_PERFORMANCE).nth(1))
    {
        found.count += 1;
        let digits: String = rest
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == ',')
            .filter(char::is_ascii_digit)
            .collect();
        if let Ok(us) = digits.parse::<u64>() {
            found.worst_us = found.worst_us.max(us);
        }
    }
    found
}

/// Counters over one period.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Minute {
    /// Wall time, in seconds.
    pub(crate) seconds: f64,
    /// CPU of the process, in 100 ns units.
    pub(crate) process: Option<u64>,
    /// CPU of the main thread.
    pub(crate) main: Option<u64>,
    /// CPU of the server loop's thread.
    pub(crate) server: Option<u64>,
    /// Server loop passes.
    pub(crate) passes: u64,
    /// Passes whose wait the cap lengthened.
    pub(crate) capped: u64,
    /// Simulation steps.
    pub(crate) steps: u64,
    /// Exact waits slept.
    pub(crate) sleeps: u64,
    /// Nanoseconds they slept.
    pub(crate) slept_ns: u64,
    /// Nanoseconds they woke past the deadline.
    pub(crate) late_ns: u64,
    /// The server's warnings.
    pub(crate) warnings: Option<Warnings>,
    /// The busiest groups of threads, busiest first.
    pub(crate) threads: Vec<Group>,
}

/// Threads that share a name but for a number, such as the task workers.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Group {
    /// The name without its number.
    pub(crate) name: String,
    /// How many threads.
    pub(crate) count: u32,
    /// Their CPU, in 100 ns units.
    pub(crate) cpu: u64,
}

/// The group a thread named `name` belongs to: the name without a trailing
/// number and the engine's `keen ` prefix.
#[must_use]
pub(crate) fn group_name(name: &str) -> &str {
    let name = name.trim();
    let name = name.strip_prefix("keen ").unwrap_or(name);
    let name = name
        .trim_end_matches(|c: char| c.is_ascii_digit())
        .trim_end();
    if name.is_empty() { "unnamed" } else { name }
}

/// The `most` busiest groups of `threads`, `(name, cpu)` each, busiest first;
/// idle groups are left out.
#[must_use]
pub(crate) fn groups<'a>(
    threads: impl IntoIterator<Item = (&'a str, u64)>,
    most: usize,
) -> Vec<Group> {
    let mut found: Vec<Group> = Vec::new();
    for (name, cpu) in threads {
        let name = group_name(name);
        if let Some(group) = found.iter_mut().find(|g| g.name == name) {
            group.count += 1;
            group.cpu += cpu;
        } else {
            found.push(Group {
                name: name.to_owned(),
                count: 1,
                cpu,
            });
        }
    }
    found.retain(|g| g.cpu > 0);
    found.sort_by(|a, b| b.cpu.cmp(&a.cpu).then_with(|| a.name.cmp(&b.name)));
    found.truncate(most);
    found
}

/// CPU in 100 ns units over `seconds` as a share of one core, in percent.
#[must_use]
pub(crate) fn percent(cpu: u64, seconds: f64) -> f64 {
    if seconds <= 0.0 {
        return 0.0;
    }
    #[expect(clippy::cast_precision_loss, reason = "a share to one decimal")]
    let cpu = cpu as f64;
    cpu / 100_000.0 / seconds
}

/// A count over `seconds` as a rate a second.
fn rate(count: u64, seconds: f64) -> f64 {
    #[expect(clippy::cast_precision_loss, reason = "a rate to one decimal")]
    let count = count as f64;
    if seconds > 0.0 { count / seconds } else { 0.0 }
}

/// The line for one period.
#[must_use]
pub(crate) fn line(minute: &Minute) -> String {
    let s = minute.seconds;
    let cpu = |value: Option<u64>| {
        value.map_or_else(|| "?".to_owned(), |v| format!("{:.1}%", percent(v, s)))
    };
    let mut out = format!(
        "CPU: process {}, main thread {}, server loop thread {} (100% = one core); \
         server loop {:.0}/s, steps {:.1}/s",
        cpu(minute.process),
        cpu(minute.main),
        cpu(minute.server),
        rate(minute.passes, s),
        rate(minute.steps, s),
    );
    if minute.capped > 0 {
        let _ = write!(out, ", {:.0}/s waits capped", rate(minute.capped, s));
    }
    let _ = write!(out, "; exact waits {:.0}/s", rate(minute.sleeps, s));
    if minute.sleeps > 0 {
        #[expect(clippy::cast_precision_loss, reason = "a mean to one decimal")]
        let mean = minute.slept_ns as f64 / minute.sleeps as f64 / 1_000.0;
        #[expect(clippy::cast_precision_loss, reason = "a mean to one decimal")]
        let late = minute.late_ns as f64 / minute.sleeps as f64 / 1_000.0;
        let _ = write!(out, " of {mean:.0} µs on average, {late:.0} µs late");
    }
    match minute.warnings {
        Some(found) if found.count > 0 => {
            let _ = write!(
                out,
                "; Bad Performance {} (worst {:.1} ms)",
                found.count,
                rate(found.worst_us, 1_000.0)
            );
        }
        Some(_) => out.push_str("; Bad Performance 0"),
        None => {}
    }
    if !minute.threads.is_empty() {
        out.push_str("; threads:");
        for (i, group) in minute.threads.iter().enumerate() {
            let sep = if i == 0 { " " } else { ", " };
            let _ = write!(out, "{sep}{}", group.name);
            if group.count > 1 {
                let _ = write!(out, " x{}", group.count);
            }
            let _ = write!(out, " {:.1}%", percent(group.cpu, s));
        }
    }
    out
}

/// Where the counters stood at the start of a period.
#[derive(Debug)]
struct Mark {
    at: Instant,
    process: Option<u64>,
    main: Option<u64>,
    server: Option<u64>,
    passes: u64,
    capped: u64,
    steps: u64,
    sleeps: u64,
    slept_ns: u64,
    late_ns: u64,
}

/// A thread followed across periods.
#[derive(Debug)]
struct Followed {
    thread: Thread,
    name: String,
    cpu: u64,
}

/// The measuring thread's state.
#[derive(Debug)]
struct Meter {
    main: Option<Thread>,
    server: Option<Thread>,
    log: Option<(PathBuf, u64)>,
    mark: Mark,
    threads: HashMap<u32, Followed>,
}

/// The game's own log: `logs/<exe name>.log` beside the game.
fn game_log() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let stem = exe.file_stem()?.to_owned();
    let mut path = exe.parent()?.join("logs").join(stem);
    path.set_extension("log");
    path.exists().then_some(path)
}

/// Everything the game logged so far, up to [`MOST_READ`], for the start
/// report.
pub(crate) fn game_log_text() -> String {
    game_log()
        .and_then(|path| read_from(&path, 0).map(|(text, _)| text))
        .unwrap_or_default()
}

/// What `path` holds from `offset`, and where it ends; from the start again
/// when the file is shorter than `offset`, as after a restart.
fn read_from(path: &Path, offset: u64) -> Option<(String, u64)> {
    let mut file = std::fs::File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    let from = if len < offset {
        0
    } else {
        offset.max(len.saturating_sub(MOST_READ))
    };
    let _at = file.seek(SeekFrom::Start(from)).ok()?;
    let mut bytes = Vec::new();
    let _read = file.take(len - from).read_to_end(&mut bytes).ok()?;
    // A line the game is still writing waits for the next read whole, or a
    // warning cut in two would match neither half.
    let complete = bytes
        .iter()
        .rposition(|&b| b == b'\n')
        .map_or(0, |at| at + 1);
    bytes.truncate(complete);
    Some((
        String::from_utf8_lossy(&bytes).into_owned(),
        from + u64::try_from(complete).unwrap_or(0),
    ))
}

impl Meter {
    fn new() -> Self {
        let log = game_log().map(|path| {
            let len = std::fs::metadata(&path).map_or(0, |m| m.len());
            (path, len)
        });
        let main = sys::main_thread();
        let mut meter = Self {
            mark: Self::mark(main.as_ref(), None),
            main,
            server: None,
            log,
            threads: HashMap::new(),
        };
        let _first = meter.thread_groups();
        meter
    }

    /// The CPU each group of threads used since the last call; a thread
    /// first seen counts from its start, one that ended drops out.
    fn thread_groups(&mut self) -> Vec<Group> {
        let main = self.main.as_ref().map(|t| t.id);
        let ids = sys::thread_ids();
        self.threads.retain(|id, _| ids.contains(id));
        let mut used: Vec<(u32, u64)> = Vec::with_capacity(ids.len());
        for id in ids {
            let followed = match self.threads.entry(id) {
                std::collections::hash_map::Entry::Occupied(e) => e.into_mut(),
                std::collections::hash_map::Entry::Vacant(e) => {
                    let Some(thread) = Thread::open(id) else {
                        continue;
                    };
                    e.insert(Followed {
                        thread,
                        name: String::new(),
                        cpu: 0,
                    })
                }
            };
            if followed.name.is_empty() {
                followed.name = if Some(id) == main {
                    "main".to_owned()
                } else {
                    followed.thread.name()
                };
            }
            let Some(cpu) = followed.thread.cpu() else {
                continue;
            };
            used.push((id, cpu.saturating_sub(followed.cpu)));
            followed.cpu = cpu;
        }
        let threads = &self.threads;
        groups(
            used.iter()
                .filter_map(|(id, cpu)| Some((threads.get(id)?.name.as_str(), *cpu))),
            THREAD_GROUPS,
        )
    }

    fn mark(main: Option<&Thread>, server: Option<&Thread>) -> Mark {
        Mark {
            at: Instant::now(),
            process: sys::process_cpu(),
            main: main.and_then(Thread::cpu),
            server: server.and_then(Thread::cpu),
            passes: PASSES.load(Ordering::Relaxed),
            capped: CAPPED.load(Ordering::Relaxed),
            steps: STEPS.load(Ordering::Relaxed),
            sleeps: SLEEPS.load(Ordering::Relaxed),
            slept_ns: SLEPT_NS.load(Ordering::Relaxed),
            late_ns: LATE_NS.load(Ordering::Relaxed),
        }
    }

    /// Opens the server loop's thread once it is known or when it changed,
    /// as when a client hosts a new game.
    fn follow_server(&mut self) -> bool {
        let id = SERVER_THREAD.load(Ordering::Relaxed);
        if id == 0 || self.server.as_ref().is_some_and(|t| t.id == id) {
            return false;
        }
        self.server = Thread::open(id);
        true
    }

    fn warnings(&mut self) -> Option<Warnings> {
        if self.log.is_none() {
            self.log = game_log().map(|path| (path, 0));
        }
        let (path, offset) = self.log.as_mut()?;
        let (text, end) = read_from(path, *offset)?;
        *offset = end;
        Some(warnings(&text))
    }

    /// Closes a period and starts the next.
    fn minute(&mut self) -> Minute {
        let fresh_server = self.follow_server();
        let warnings = self.warnings();
        let threads = self.thread_groups();
        let now = Self::mark(self.main.as_ref(), self.server.as_ref());
        let was = &self.mark;
        let delta = |a: Option<u64>, b: Option<u64>| Some(a?.saturating_sub(b?));
        let minute = Minute {
            seconds: now.at.duration_since(was.at).as_secs_f64(),
            process: delta(now.process, was.process),
            main: delta(now.main, was.main),
            // A thread first seen this period counts from its start.
            server: if fresh_server {
                now.server
            } else {
                delta(now.server, was.server)
            },
            passes: now.passes - was.passes,
            capped: now.capped - was.capped,
            steps: now.steps - was.steps,
            sleeps: now.sleeps - was.sleeps,
            slept_ns: now.slept_ns - was.slept_ns,
            late_ns: now.late_ns - was.late_ns,
            warnings,
            threads,
        };
        self.mark = now;
        minute
    }
}

/// Measures until the process ends: one line every [`PERIOD`] while measure
/// mode is on. On Windows it also keeps the exact waits' slack at the timer
/// resolution the game set.
pub(crate) fn run(wine: bool) {
    let mut meter = Meter::new();
    let mut workers_told = false;
    loop {
        if !wine {
            waits::tune_for_windows();
        }
        std::thread::sleep(PERIOD);
        let minute = meter.minute();
        if !workers_told {
            let game = crate::env::parse_game_log(&game_log_text());
            if !game.workers.is_empty() {
                workers_told = true;
                enshwick::info!("{}", game.started());
            }
        }
        if ON.load(Ordering::Relaxed) {
            let text = format!("{}; {}", context(), line(&minute));
            enshwick::info!("measure: {text}");
            *LAST.lock().unwrap_or_else(PoisonError::into_inner) = text;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_warning_cut_by_a_read_counts_once_it_is_whole() {
        let path =
            std::env::temp_dir().join(format!("enshwick-optimizer-log-{}", std::process::id()));
        std::fs::write(&path, "[W 00:10:02,000] [server] Bad Perf").unwrap();
        let (text, at) = read_from(&path, 0).unwrap();
        assert_eq!((text.as_str(), at), ("", 0), "the half line waits");
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        std::io::Write::write_all(&mut file, b"ormance 900 us (peak: 900 us)\n").unwrap();
        drop(file);
        let (text, _) = read_from(&path, at).unwrap();
        assert_eq!(warnings(&text).count, 1);
        let _gone = std::fs::remove_file(&path);
    }

    #[test]
    fn turning_measure_off_drops_the_last_line() {
        LAST.lock().unwrap().push_str("old");
        set(false, String::new());
        assert!(last().is_empty());
    }

    #[test]
    fn warnings_are_counted_with_the_worst_pass() {
        let log = "\
[I 00:10:00,000] [server] Bad Performance 24,700 us (peak: 34,012 us)
[I 00:10:01,000] [session] something else
[W 00:10:02,000] [server] Bad Performance 1,234,567 us (peak: 1,234,567 us)
[W 00:10:03,000] [server] Bad Performance 900 us (peak: 900 us)
";
        assert_eq!(
            warnings(log),
            Warnings {
                count: 3,
                worst_us: 1_234_567
            }
        );
        assert_eq!(warnings(""), Warnings::default());
    }

    #[test]
    fn cpu_is_a_share_of_one_core() {
        // 30 s of CPU in 60 s is half a core.
        assert!((percent(300_000_000, 60.0) - 50.0).abs() < 1e-9);
        assert!((percent(1_200_000_000, 60.0) - 200.0).abs() < 1e-9);
        assert!(percent(5, 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn the_line_names_every_counter() {
        let minute = Minute {
            seconds: 60.0,
            process: Some(300_000_000),
            main: Some(30_000_000),
            server: None,
            passes: 54_000,
            capped: 0,
            steps: 3_600,
            sleeps: 3_600,
            slept_ns: 3_600 * 600_000,
            late_ns: 0,
            warnings: Some(Warnings {
                count: 2,
                worst_us: 34_012,
            }),
            threads: groups([("keen Session", 120_000_000), ("main", 30_000_000)], 6),
        };
        assert_eq!(
            line(&minute),
            "CPU: process 50.0%, main thread 5.0%, server loop thread ? (100% = one core); \
             server loop 900/s, steps 60.0/s; exact waits 60/s of 600 µs on average, 0 µs late; \
             Bad Performance 2 (worst 34.0 ms); threads: Session 20.0%, main 5.0%"
        );
        let quiet = Minute {
            seconds: 60.0,
            capped: 600,
            ..Minute::default()
        };
        assert_eq!(
            line(&quiet),
            "CPU: process ?, main thread ?, server loop thread ? (100% = one core); \
             server loop 0/s, steps 0.0/s, 10/s waits capped; exact waits 0/s"
        );
    }

    #[test]
    fn threads_group_by_name_without_their_number() {
        assert_eq!(group_name("keen Task-Default07"), "Task-Default");
        assert_eq!(group_name("CNet Encrypt:0"), "CNet Encrypt:");
        assert_eq!(group_name("  "), "unnamed");
        assert_eq!(group_name("main"), "main");
        let found = groups(
            [
                ("keen Task-Default00", 10),
                ("keen Task-Default01", 30),
                ("keen GameServer", 25),
                ("idle", 0),
                ("keen Session", 5),
            ],
            2,
        );
        assert_eq!(
            found,
            [
                Group {
                    name: "Task-Default".into(),
                    count: 2,
                    cpu: 40
                },
                Group {
                    name: "GameServer".into(),
                    count: 1,
                    cpu: 25
                },
            ]
        );
    }

    #[test]
    fn the_log_is_read_on_from_where_it_stopped() {
        let dir = std::env::temp_dir().join(format!("optimizer-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("game.log");
        std::fs::write(&path, "one\n").unwrap();
        let (text, end) = read_from(&path, 0).unwrap();
        assert_eq!((text.as_str(), end), ("one\n", 4));
        std::fs::write(&path, "one\ntwo\n").unwrap();
        assert_eq!(read_from(&path, end).unwrap().0, "two\n");
        std::fs::write(&path, "x\n").unwrap();
        assert_eq!(
            read_from(&path, 8).unwrap().0,
            "x\n",
            "a new log starts over"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
