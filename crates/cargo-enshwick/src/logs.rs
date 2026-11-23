//! `cargo enshwick logs`: the newest run's log files in a game folder, read
//! whole or followed as the loader writes them, merged by time.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{ErrorKind, Read as _, Seek as _, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context as _, bail};
use enshwick_base::log_files::{self, Line, run_file};
use enshwick_core::abi::Level;

/// The loader's source, the name its run files start with.
pub(crate) use enshwick_base::mods::LOADER_ID as LOADER;
pub(crate) use log_files::LOGS;

/// The bytes at the end of a loader file read for its
/// [`log_files::rolled`] line.
const TAIL: u64 = 512;
/// The least drop in the time of day, in milliseconds, between two lines of
/// one file that is a new day: a smaller one is lines stamped on other
/// threads and written a little out of order.
const NEXT_DAY_DROP: u32 = 60 * 60 * 1000;
/// How often a followed log is read again.
pub(crate) const POLL: Duration = Duration::from_millis(250);

/// The day and the milliseconds into it of a run file's stamp, which the
/// loader writes in UTC, as it does its lines' times.
fn stamp_at(stamp: &str) -> Option<(u32, u32)> {
    let part = |range: std::ops::Range<usize>| stamp.get(range)?.parse::<u32>().ok();
    let day = civil_day(part(0..4)?, part(5..7)?, part(8..10)?)?;
    let seconds = (part(11..13)? * 60 + part(14..16)?) * 60 + part(17..19)?;
    Some((day, seconds * 1000))
}

/// A count of days that orders proleptic Gregorian dates, from 0000-03-01
/// (Howard Hinnant's `days_from_civil`).
fn civil_day(year: u32, month: u32, day: u32) -> Option<u32> {
    let year = year.checked_sub(u32::from(month <= 2))?;
    let year_of_era = year % 400;
    let day_of_year = (153 * ((month + 9) % 12) + 2) / 5 + day.checked_sub(1)?;
    Some(
        year / 400 * 146_097 + year_of_era * 365 + year_of_era / 4 - year_of_era / 100
            + day_of_year,
    )
}

/// One entry of a run file: a [`Line`] with the lines that continue it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Entry {
    /// The day, counted from the file's stamp, and the milliseconds into it,
    /// both UTC: what entries of several files sort by.
    pub(crate) at: (u32, u32),
    /// `None` for lines before the first entry, such as the stub's.
    pub(crate) level: Option<Level>,
    pub(crate) source: String,
    /// The message after the source, its continuation lines included.
    pub(crate) message: String,
    /// The entry as the file holds it.
    pub(crate) text: String,
}

impl Entry {
    /// The entry `line` opens, on day 0 until its file counts the day;
    /// `None` for a line that continues the one before.
    fn parse(line: &str) -> Option<Self> {
        let parsed = Line::parse(line)?;
        Some(Self {
            at: (0, parsed.millis),
            level: Some(parsed.level),
            source: parsed.source.to_owned(),
            message: parsed.message.to_owned(),
            text: line.to_owned(),
        })
    }

    /// Whether `filter` shows the entry.
    pub(crate) fn shown(&self, filter: &Filter) -> bool {
        self.level.is_none_or(|level| level <= filter.level)
            && filter.source.as_ref().is_none_or(|s| *s == self.source)
    }
}

/// Which entries are shown: up to a level, of one source or all.
#[derive(Debug, Clone)]
pub(crate) struct Filter {
    pub(crate) level: Level,
    pub(crate) source: Option<String>,
}

/// A followed file: the bytes read and what the last read left.
#[derive(Debug)]
struct Followed {
    read: u64,
    /// The end of a line the loader has not finished writing.
    partial: Vec<u8>,
    /// The entry given last, whose day, time, level and source a line that
    /// continues it in a later read takes.
    last: Option<Entry>,
    /// The day and time the file's lines have reached, from its stamp on.
    at: (u32, u32),
}

impl Followed {
    /// A file not read yet, whose run or roll-over opened it at `stamp`.
    fn new(stamp: &str) -> Self {
        Self {
            read: 0,
            partial: Vec::new(),
            last: None,
            at: stamp_at(stamp).unwrap_or_default(),
        }
    }
}

/// The run files of a log folder, read as they grow.
#[derive(Debug)]
pub(crate) struct Follower {
    dir: PathBuf,
    /// Files left alone: those already there when a run started.
    old: BTreeSet<String>,
    /// The stamp of the first run file followed, when a run is named by it.
    from: Option<String>,
    files: BTreeMap<String, Followed>,
}

impl Follower {
    /// Follows the files a run started after now writes in `dir`.
    pub(crate) fn new_run(dir: &Path) -> Self {
        let old = names(dir).into_iter().collect();
        Self {
            dir: dir.to_owned(),
            old,
            from: None,
            files: BTreeMap::new(),
        }
    }

    /// Follows the newest run in `dir`, from the stamp every source's first
    /// file of it shares on: the loader's newest file that no other one
    /// names as where the run went on after its size cap.
    ///
    /// # Errors
    ///
    /// When `dir` holds no file of the loader.
    pub(crate) fn newest_run(dir: &Path) -> anyhow::Result<Self> {
        let loader: Vec<String> = names(dir)
            .into_iter()
            .filter(|name| run_file(name).is_some_and(|(source, _)| source == LOADER))
            .collect();
        let rolled: BTreeSet<String> = loader
            .iter()
            .filter_map(|name| rolled_into(&dir.join(name)))
            .collect();
        // shortcut: a run that rolled its loader file more than the loader keeps has lost its first file, so its sources' first files are left out; upgrade when runs that long need the merged view.
        let newest = loader
            .iter()
            .filter(|name| !rolled.contains(*name))
            .filter_map(|name| run_file(name).map(|(_, stamp)| stamp.to_owned()))
            .max();
        let Some(from) = newest else {
            bail!(
                "{} holds no log of the loader: start the game or server with Enshwick first",
                dir.display()
            );
        };
        Ok(Self {
            dir: dir.to_owned(),
            old: BTreeSet::new(),
            from: Some(from),
            files: BTreeMap::new(),
        })
    }

    /// The entries written whole since the last call, in order of time.
    ///
    /// # Errors
    ///
    /// When a followed file that is still there cannot be read.
    pub(crate) fn poll(&mut self) -> anyhow::Result<Vec<Entry>> {
        for name in names(&self.dir) {
            let Some((_, stamp)) = run_file(&name) else {
                continue;
            };
            let ours =
                !self.old.contains(&name) && self.from.as_deref().is_none_or(|from| stamp >= from);
            if ours && !self.files.contains_key(&name) {
                let followed = Followed::new(stamp);
                self.files.insert(name, followed);
            }
        }
        let mut entries = Vec::new();
        let mut gone = Vec::new();
        for (name, followed) in &mut self.files {
            let path = self.dir.join(name);
            let (source, stamp) = run_file(name).unwrap_or_default();
            let mut file = match std::fs::File::open(&path) {
                Ok(file) => file,
                // The loader deletes a source's oldest files as it rolls
                // over, and deleting one by hand is safe.
                Err(e) if e.kind() == ErrorKind::NotFound => {
                    gone.push(name.clone());
                    continue;
                }
                Err(e) => return Err(e).with_context(|| path.display().to_string()),
            };
            let length = file.metadata().map_or(0, |m| m.len());
            if length < followed.read {
                // Deleted and made again by hand: read it from the start.
                *followed = Followed::new(stamp);
            }
            let taken = file
                .seek(SeekFrom::Start(followed.read))
                .and_then(|_| file.read_to_end(&mut followed.partial))
                .with_context(|| path.display().to_string())?;
            followed.read += taken as u64;
            take_lines(followed, source, &mut entries);
        }
        for name in gone {
            self.files.remove(&name);
        }
        entries.sort_by_key(|entry| entry.at);
        Ok(entries)
    }
}

/// The file the loader's file at `path` says its run went on in once it
/// reached its size cap; `None` for a file that did not.
fn rolled_into(path: &Path) -> Option<String> {
    let mut file = std::fs::File::open(path).ok()?;
    let length = file.metadata().ok()?.len();
    file.seek(SeekFrom::Start(length.saturating_sub(TAIL)))
        .ok()?;
    let mut tail = Vec::new();
    file.read_to_end(&mut tail).ok()?;
    log_files::rolled_into(&String::from_utf8_lossy(&tail)).map(str::to_owned)
}

/// Moves the whole lines of `followed` into entries. The loader writes an
/// entry and its continuation lines at once, so each read ends on a whole
/// entry; a continuation line a read still splits off takes the entry before
/// it's day, time, level and source.
fn take_lines(followed: &mut Followed, source: &str, entries: &mut Vec<Entry>) {
    let Some(end) = followed.partial.iter().rposition(|&b| b == b'\n') else {
        return;
    };
    let whole: Vec<u8> = followed.partial.drain(..=end).collect();
    let mut open: Option<Entry> = None;
    for line in String::from_utf8_lossy(&whole).lines() {
        let line = line.trim_end_matches('\r');
        match (Entry::parse(line), &mut open) {
            (Some(mut entry), open) => {
                // shortcut: a file silent for nearly a day reads its next line as the same day; upgrade when gaps that long need the merged view.
                let (day, time) = followed.at;
                entry.at.0 = day + u32::from(time > entry.at.1 + NEXT_DAY_DROP);
                followed.at = entry.at;
                entries.extend(open.replace(entry));
            }
            (None, Some(open)) => {
                open.text.push('\n');
                open.text.push_str(line);
                open.message.push('\n');
                open.message.push_str(line);
            }
            (None, None) => {
                open = Some(match &followed.last {
                    Some(before) => Entry {
                        message: line.to_owned(),
                        text: line.to_owned(),
                        ..before.clone()
                    },
                    None => Entry {
                        at: followed.at,
                        level: None,
                        source: source.to_owned(),
                        message: line.to_owned(),
                        text: line.to_owned(),
                    },
                });
            }
        }
    }
    if let Some(open) = open {
        followed.last = Some(open.clone());
        entries.push(open);
    }
}

/// The file names in `dir`, none when it does not read.
fn names(dir: &Path) -> Vec<String> {
    std::fs::read_dir(dir).map_or_default(|entries| {
        entries
            .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
            .collect()
    })
}

/// What `cargo enshwick logs` was asked for.
#[derive(Debug, clap::Args)]
pub(crate) struct Args {
    /// The game or dedicated server folder; `ENSHROUDED_DIR` when omitted,
    /// else Steam's dedicated server, else Steam's game.
    #[arg(long, env = "ENSHROUDED_DIR")]
    game: Option<PathBuf>,
    /// Goes on printing new lines until Ctrl-C.
    #[arg(long, short)]
    follow: bool,
    /// Prints only this mod's lines, by its id.
    #[arg(long = "mod", value_name = "ID")]
    mod_id: Option<String>,
    /// The most detailed lines printed: error, warn, info or debug. The
    /// server's `enshwick.toml` decides which lines reach the files.
    #[arg(long, default_value = "debug")]
    level: Level,
}

/// Prints the newest run's lines `args` asks for to `out`.
pub(crate) fn run(args: &Args, out: &mut impl Write) -> anyhow::Result<()> {
    let game = if let Some(game) = &args.game {
        game.clone()
    } else {
        let mut found = crate::steam::find(&crate::steam::steam_folders());
        found.sort_by_key(|f| f.app != crate::steam::SERVER_APP);
        found.into_iter().next().map(|f| f.folder).context(
            "Steam has neither the Enshrouded dedicated server nor the game installed here; name the folder with --game or ENSHROUDED_DIR",
        )?
    };
    let filter = Filter {
        level: args.level,
        source: args.mod_id.clone(),
    };
    show(&game, args.follow, &filter, out)
}

/// Writes the newest run's entries in `game`'s logs that `filter` shows,
/// then with `follow` every new one until the process is stopped.
///
/// # Errors
///
/// When the logs hold no run of the loader or a file does not read.
pub(crate) fn show(
    game: &Path,
    follow: bool,
    filter: &Filter,
    out: &mut impl Write,
) -> anyhow::Result<()> {
    let mut follower = Follower::newest_run(&game.join(LOGS))?;
    loop {
        for entry in follower.poll()? {
            if entry.shown(filter) {
                writeln!(out, "{}", entry.text)?;
            }
        }
        if !follow {
            return Ok(());
        }
        out.flush()?;
        std::thread::sleep(POLL);
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A line as the loader writes it at `time`, `HH:MM:SS.mmm`.
    pub(crate) fn line(time: &str, level: Level, source: &str, message: &str) -> String {
        let (clock, millis) = time.split_once('.').unwrap();
        let seconds = clock
            .split(':')
            .fold(0, |sum, part| sum * 60 + part.parse::<u64>().unwrap());
        let mut text = String::new();
        let at = seconds * 1000 + millis.parse::<u64>().unwrap();
        log_files::write_line(&mut text, at, level, source, message);
        text
    }

    fn append(path: &Path, text: &str) {
        use std::io::Write as _;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .unwrap();
        file.write_all(text.as_bytes()).unwrap();
    }

    #[test]
    fn a_stamp_reads_as_a_day_that_orders_dates_and_its_time() {
        let at = |stamp| stamp_at(stamp).unwrap();
        assert_eq!(at("2026-11-03_10-00-00").1, 36_000_000);
        assert_eq!(at("2026-11-04_00-00-00").0, at("2026-11-03_23-59-59").0 + 1);
        assert_eq!(at("2026-03-01_00-00-00").0, at("2026-02-28_00-00-00").0 + 1);
        assert_eq!(at("2028-03-01_00-00-00").0, at("2028-02-28_00-00-00").0 + 2);
        assert_eq!(at("2027-01-01_00-00-00").0, at("2026-12-31_00-00-00").0 + 1);
        assert_eq!(stamp_at("2026-11-03"), None);
    }

    #[test]
    fn lines_a_little_out_of_order_keep_their_day_and_a_late_first_line_takes_the_next() {
        let dir = crate::scratch("days");
        let loader = dir.join("enshwick_2026-11-03_23-50-00.log");
        append(&loader, &line("23:55:00.101", Level::INFO, LOADER, "a"));
        append(
            &loader,
            &line(
                "23:55:00.100",
                Level::WARN,
                LOADER,
                "3 log lines were dropped",
            ),
        );
        append(&loader, &line("23:56:00.000", Level::INFO, LOADER, "b"));
        append(
            &dir.join("you.mod_2026-11-03_23-50-00.log"),
            &line(
                "00:05:00.000",
                Level::INFO,
                "you.mod",
                "first, after midnight",
            ),
        );
        let entries = Follower::newest_run(&dir).unwrap().poll().unwrap();
        let messages: Vec<&str> = entries.iter().map(|e| e.message.as_str()).collect();
        assert_eq!(
            messages,
            [
                "3 log lines were dropped",
                "a",
                "b",
                "first, after midnight"
            ]
        );
        assert_eq!(entries[0].at.0, entries[2].at.0, "{entries:?}");
        assert_eq!(entries[3].at.0, entries[2].at.0 + 1, "{entries:?}");
        let _gone = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_run_whose_loader_file_rolled_over_keeps_every_source_s_lines() {
        let dir = crate::scratch("rolled");
        append(
            &dir.join("enshwick_2026-11-02_10-00-00.log"),
            &line("10:00:00.000", Level::INFO, LOADER, "old run"),
        );
        let first = dir.join("enshwick_2026-11-03_10-00-00.log");
        append(
            &first,
            &line("10:00:00.000", Level::INFO, LOADER, "starting"),
        );
        append(
            &first,
            &line(
                "12:00:00.000",
                Level::INFO,
                LOADER,
                &log_files::rolled("enshwick_2026-11-03_12-00-01.log"),
            ),
        );
        append(
            &dir.join("enshwick_2026-11-03_12-00-01.log"),
            &line("12:00:00.500", Level::INFO, LOADER, "went on"),
        );
        append(
            &dir.join("you.mod_2026-11-03_10-00-00.log"),
            &line("10:30:00.000", Level::INFO, "you.mod", "hello"),
        );
        let entries = Follower::newest_run(&dir).unwrap().poll().unwrap();
        let messages: Vec<&str> = entries.iter().map(|e| e.message.as_str()).collect();
        assert_eq!(messages[..2], ["starting", "hello"]);
        assert_eq!(messages[3], "went on");
        assert!(!messages.contains(&"old run"), "{messages:?}");
        let _gone = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_followed_file_that_is_deleted_is_left_and_the_rest_go_on() {
        let dir = crate::scratch("deleted");
        let mut follower = Follower::new_run(&dir);
        let gone = dir.join("you.mod_2026-11-03_10-00-00.log");
        let loader = dir.join("enshwick_2026-11-03_10-00-00.log");
        append(&gone, &line("10:00:00.000", Level::INFO, "you.mod", "a"));
        assert_eq!(follower.poll().unwrap().len(), 1);
        std::fs::remove_file(&gone).unwrap();
        append(&loader, &line("10:00:01.000", Level::INFO, LOADER, "b"));
        let entries = follower.poll().unwrap();
        assert_eq!(entries.len(), 1, "{entries:?}");
        assert_eq!(entries[0].message, "b");
        let _gone = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_newest_run_is_merged_by_time_and_filtered() {
        let game = crate::scratch("newest");
        let all = Filter {
            level: Level::INFO,
            source: None,
        };
        let none = show(&game, false, &all, &mut Vec::new()).unwrap_err();
        assert!(none.to_string().contains("no log of the loader"), "{none}");
        let dir = game.join(LOGS);
        std::fs::create_dir(&dir).unwrap();
        let old = dir.join("enshwick_2026-11-02_10-00-00.log");
        std::fs::write(
            &old,
            line("10:00:00.000", Level::INFO, "enshwick", "old run"),
        )
        .unwrap();
        let loader = dir.join("enshwick_2026-11-03_10-00-00.log");
        let pack = dir.join("you.pack_2026-11-03_10-00-00.log");
        std::fs::write(&loader, "stub: started\n").unwrap();
        append(
            &loader,
            &line("10:00:00.000", Level::INFO, "enshwick", "starting"),
        );
        append(
            &loader,
            &line(
                "10:00:02.000",
                Level::INFO,
                "enshwick",
                "ready: 0 of 1 mods running",
            ),
        );
        std::fs::write(
            &pack,
            line("10:00:01.000", Level::ERROR, "you.pack", "off: no item"),
        )
        .unwrap();
        append(&pack, "  rewards.toml:15:1\n");
        let mut out = Vec::new();
        show(&game, false, &all, &mut out).unwrap();
        let out = String::from_utf8(out).unwrap();
        assert!(!out.contains("old run"), "{out}");
        assert!(out.contains("  rewards.toml:15:1\n"), "{out}");
        let mut follower = Follower::newest_run(&dir).unwrap();
        let entries = follower.poll().unwrap();
        let messages: Vec<&str> = entries.iter().map(|e| e.message.as_str()).collect();
        assert_eq!(
            messages,
            [
                "stub: started",
                "starting",
                "off: no item\n  rewards.toml:15:1",
                "ready: 0 of 1 mods running"
            ]
        );
        let warn = Filter {
            level: Level::WARN,
            source: Some("you.pack".to_owned()),
        };
        let shown: Vec<&Entry> = entries.iter().filter(|e| e.shown(&warn)).collect();
        assert_eq!(shown.len(), 1);
        assert_eq!(shown[0].source, "you.pack");
        let _gone = std::fs::remove_dir_all(&game);
    }

    #[test]
    fn a_followed_run_gives_each_entry_once_it_is_whole() {
        let dir = crate::scratch("follow");
        std::fs::write(dir.join("enshwick_2026-11-02_10-00-00.log"), "before\n").unwrap();
        let mut follower = Follower::new_run(&dir);
        let loader = dir.join("enshwick_2026-11-03_10-00-00.log");
        append(
            &loader,
            &line("23:59:59.000", Level::INFO, "enshwick", "first"),
        );
        append(&loader, "00:00:01.000Z info  [enshwick] late");
        let first = follower.poll().unwrap();
        assert_eq!(first.len(), 1, "a whole line is given at once");
        assert_eq!(first[0].message, "first");
        append(&loader, "r\n  where it went on\n");
        let second = follower.poll().unwrap();
        assert_eq!(second.len(), 1, "{second:?}");
        assert_eq!(second[0].message, "later\n  where it went on");
        assert_eq!(second[0].at.0, first[0].at.0 + 1, "after midnight");
        append(&loader, "  in a later write\n");
        let third = follower.poll().unwrap();
        assert_eq!(third[0].message, "  in a later write");
        assert_eq!(third[0].source, "enshwick");
        assert_eq!(third[0].level, Some(Level::INFO));
        assert_eq!(third[0].at, second[0].at);
        assert!(follower.poll().unwrap().is_empty());
        let _gone = std::fs::remove_dir_all(&dir);
    }
}
