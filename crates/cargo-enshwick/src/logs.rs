//! `cargo enshwick logs`: the newest run's log files in a game folder, read
//! whole or followed as the loader writes them, merged by time.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read as _, Seek as _, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context as _, bail};
use enshwick_core::abi::Level;

/// The folder beside the game's executable that holds every log.
pub(crate) const LOGS: &str = "logs";
/// The loader's source, the name its run files start with.
pub(crate) const LOADER: &str = "enshwick";
/// The length of a run file's stamp, `2026-11-03_10-00-00`.
const STAMP_LEN: usize = 19;
/// The levels, most severe first, as log lines spell them.
const LEVELS: [Level; 4] = [Level::ERROR, Level::WARN, Level::INFO, Level::DEBUG];
/// How often a followed log is read again.
pub(crate) const POLL: Duration = Duration::from_millis(250);

/// The source and stamp of a file the loader writes for one source and run,
/// `<source>_<stamp>.log`; `None` for any other file.
pub(crate) fn run_file(name: &str) -> Option<(&str, &str)> {
    let base = name.strip_suffix(".log")?;
    let (source, stamp) = base.split_at_checked(base.len().checked_sub(STAMP_LEN)?)?;
    let source = source.strip_suffix('_')?;
    let shaped = stamp.bytes().enumerate().all(|(i, b)| match i {
        4 | 7 | 13 | 16 => b == b'-',
        10 => b == b'_',
        _ => b.is_ascii_digit(),
    });
    (shaped && !source.is_empty()).then_some((source, stamp))
}

/// A level by the name log lines and settings give it.
pub(crate) fn level(name: &str) -> Result<Level, String> {
    LEVELS
        .into_iter()
        .find(|level| level.name() == name)
        .ok_or_else(|| {
            let names: Vec<&str> = LEVELS.iter().map(|level| level.name()).collect();
            format!("{name:?} is no level; one of {}", names.join(", "))
        })
}

/// One entry of a run file: a line `HH:MM:SS.mmmZ <level> [<source>] <message>`
/// with the lines that continue it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Entry {
    /// The day of the run, counted in its file, and the time of day: what
    /// entries of several files sort by.
    pub(crate) at: (u32, String),
    /// `None` for lines before the first entry, such as the stub's.
    pub(crate) level: Option<Level>,
    pub(crate) source: String,
    /// The message after the source, its continuation lines included.
    pub(crate) message: String,
    /// The entry as the file holds it.
    pub(crate) text: String,
}

impl Entry {
    /// The entry `line` opens; `None` for a line that continues the one
    /// before.
    fn parse(line: &str, day: u32) -> Option<Self> {
        let (time, rest) = line.split_once(' ')?;
        if !time.ends_with('Z') {
            return None;
        }
        let (name, rest) = rest.trim_start().split_once(' ')?;
        let level = level(name).ok()?;
        let (source, message) = rest.trim_start().strip_prefix('[')?.split_once("] ")?;
        Some(Self {
            at: (day, time.to_owned()),
            level: Some(level),
            source: source.to_owned(),
            message: message.to_owned(),
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
#[derive(Debug, Default)]
struct Followed {
    read: u64,
    /// The end of a line the loader has not finished writing.
    partial: Vec<u8>,
    /// The entry given last, whose day, time, level and source a line that
    /// continues it in a later read takes.
    last: Option<Entry>,
    /// Days the time of day went back in this file.
    day: u32,
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

    /// Follows the newest run in `dir`, from the loader's newest file on.
    ///
    /// # Errors
    ///
    /// When `dir` holds no file of the loader.
    pub(crate) fn newest_run(dir: &Path) -> anyhow::Result<Self> {
        let newest = names(dir)
            .iter()
            .filter_map(|name| run_file(name))
            .filter(|(source, _)| *source == LOADER)
            .map(|(_, stamp)| stamp.to_owned())
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
    /// When a followed file cannot be read.
    pub(crate) fn poll(&mut self) -> anyhow::Result<Vec<Entry>> {
        for name in names(&self.dir) {
            let Some((_, stamp)) = run_file(&name) else {
                continue;
            };
            let ours =
                !self.old.contains(&name) && self.from.as_deref().is_none_or(|from| stamp >= from);
            if ours && !self.files.contains_key(&name) {
                self.files.insert(name, Followed::default());
            }
        }
        let mut entries = Vec::new();
        for (name, followed) in &mut self.files {
            let path = self.dir.join(name);
            let mut file =
                std::fs::File::open(&path).with_context(|| path.display().to_string())?;
            let length = file.metadata().map_or(0, |m| m.len());
            if length < followed.read {
                // Deleted and made again by hand: read it from the start.
                *followed = Followed::default();
            }
            let taken = file
                .seek(SeekFrom::Start(followed.read))
                .and_then(|_| file.read_to_end(&mut followed.partial))
                .with_context(|| path.display().to_string())?;
            followed.read += taken as u64;
            let source = run_file(name).map_or("", |(source, _)| source);
            take_lines(followed, source, &mut entries);
        }
        entries.sort_by(|a, b| a.at.cmp(&b.at));
        Ok(entries)
    }
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
        let before = open.as_ref().or(followed.last.as_ref());
        let last_time = before.map(|entry| entry.at.1.clone());
        let continued = before.map(|entry| Entry {
            message: line.to_owned(),
            text: line.to_owned(),
            ..entry.clone()
        });
        // shortcut: a file rolled over at its size cap starts its day count again, so its lines after a midnight sort before the old file's; upgrade when runs that long need the merged view.
        let entry = Entry::parse(line, followed.day).map(|mut entry| {
            if last_time.is_some_and(|last| entry.at.1 < last) {
                followed.day += 1;
                entry.at.0 = followed.day;
            }
            entry
        });
        match (entry, &mut open) {
            (Some(entry), open) => entries.extend(open.replace(entry)),
            (None, Some(open)) => {
                open.text.push('\n');
                open.text.push_str(line);
                open.message.push('\n');
                open.message.push_str(line);
            }
            (None, None) => {
                open = Some(continued.unwrap_or_else(|| Entry {
                    at: (0, String::new()),
                    level: None,
                    source: source.to_owned(),
                    message: line.to_owned(),
                    text: line.to_owned(),
                }));
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
    /// The most detailed lines printed: error, warn, info or debug.
    #[arg(long, default_value = "debug", value_parser = level)]
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

    /// A line as the loader writes it.
    pub(crate) fn line(time: &str, level: Level, source: &str, message: &str) -> String {
        format!("{time}Z {:<5} [{source}] {message}\n", level.name())
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
    fn a_run_file_names_its_source_and_stamp() {
        assert_eq!(
            run_file("you.my-mod_2026-11-03_10-00-00.log"),
            Some(("you.my-mod", "2026-11-03_10-00-00"))
        );
        assert_eq!(run_file("start.log"), None);
        assert_eq!(run_file("_2026-11-03_10-00-00.log"), None);
        assert_eq!(run_file("enshwick_2026x11-03_10-00-00.log"), None);
    }

    #[test]
    fn a_line_reads_as_its_time_level_source_and_message() {
        let text = line(
            "00:42:35.449",
            Level::ERROR,
            "you.pack",
            "off: init returned 3",
        );
        let entry = Entry::parse(text.trim_end(), 0).unwrap();
        assert_eq!(entry.at, (0, "00:42:35.449Z".to_owned()));
        assert_eq!(entry.level, Some(Level::ERROR));
        assert_eq!(entry.source, "you.pack");
        assert_eq!(entry.message, "off: init returned 3");
        assert_eq!(Entry::parse("  at the second line", 0), None);
        assert!(
            level("loud")
                .unwrap_err()
                .contains("error, warn, info, debug")
        );
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
        assert_eq!(second[0].at.0, 1, "after midnight");
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
