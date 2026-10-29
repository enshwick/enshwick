//! The command file: lines appended to `mod-data/enshwick/commands.txt` run
//! on the server's console, each once.
//!
//! The file is read again whenever its length or modified time changes.
//! Only complete lines run, so a line still being written waits for its line
//! break. Lines already in the file when the server starts are skipped.
//!
//! The file is append-only. A file whose part already read changed counts
//! once it reads the same twice in a row, so an editor's save caught half
//! written runs nothing. An emptied file is read again from its start; any
//! other edit skips as many lines as already ran, so a kick, ban or give
//! never runs twice, and the edit is reported for a warning.

use std::hash::{DefaultHasher, Hasher};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// The command file's name, in a data folder.
pub const FILE: &str = "commands.txt";

/// How often the command files are looked at; one stat call each.
const POLL: Duration = Duration::from_secs(1);

/// Where a command file has been read up to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Tail {
    offset: usize,
    /// The complete lines up to `offset`, blank lines and comments among
    /// them.
    lines: usize,
    /// A hash of the part read, to see the file replaced.
    read: u64,
    /// A hash of the whole file last seen replaced or shorter, which waits
    /// for a second read before it counts.
    changed: Option<u64>,
    /// Whether an edit of the part read counted since the last
    /// [`Tail::take_edited`].
    edited: bool,
}

/// A hash of `bytes`, the same throughout the process.
fn hash(bytes: &[u8]) -> u64 {
    let mut hasher = DefaultHasher::new();
    hasher.write(bytes);
    hasher.finish()
}

impl Tail {
    /// A tail past the complete lines of `content`, the file at start.
    #[must_use]
    pub fn at(content: &[u8]) -> Self {
        let offset = complete(content);
        Self {
            offset,
            lines: breaks(&content[..offset]),
            read: hash(&content[..offset]),
            changed: None,
            edited: false,
        }
    }

    /// Whether a replaced or shorter file waits for another read, even if it
    /// does not change.
    #[must_use]
    pub const fn waits(&self) -> bool {
        self.changed.is_some()
    }

    /// Whether an edit of the part already read counted since the last
    /// call: lines that ran were changed or removed, so the lines past their
    /// count are new.
    pub const fn take_edited(&mut self) -> bool {
        core::mem::replace(&mut self.edited, false)
    }

    /// The console lines `content`, the whole file now, holds past the last
    /// read; moves past them.
    #[must_use]
    pub fn feed(&mut self, content: &[u8]) -> Vec<String> {
        if content
            .get(..self.offset)
            .is_none_or(|read| hash(read) != self.read)
        {
            let whole = hash(content);
            if self.changed != Some(whole) {
                self.changed = Some(whole);
                return Vec::new();
            }
            let end = complete(content);
            if end == 0 {
                self.lines = 0;
                self.offset = 0;
            } else {
                self.lines = self.lines.min(breaks(&content[..end]));
                self.offset = after_lines(content, self.lines);
                self.edited = true;
            }
            self.read = hash(&content[..self.offset]);
        }
        self.changed = None;
        let end = complete(content);
        if end <= self.offset {
            return Vec::new();
        }
        let fresh = &content[self.offset..end];
        self.lines += breaks(fresh);
        self.offset = end;
        self.read = hash(&content[..end]);
        String::from_utf8_lossy(fresh)
            .lines()
            .filter_map(line)
            .collect()
    }
}

/// The length of `content` up to and including its last line break.
fn complete(content: &[u8]) -> usize {
    content
        .iter()
        .rposition(|&b| b == b'\n')
        .map_or(0, |at| at + 1)
}

/// The line breaks in `content`.
fn breaks(content: &[u8]) -> usize {
    memchr::memchr_iter(b'\n', content).count()
}

/// The length of the first `lines` complete lines of `content`.
fn after_lines(content: &[u8], lines: usize) -> usize {
    lines.checked_sub(1).map_or(0, |last| {
        memchr::memchr_iter(b'\n', content)
            .nth(last)
            .map_or(content.len(), |at| at + 1)
    })
}

/// The console line a line of input holds, trimmed; `None` for blank lines
/// and `#` comments.
#[must_use]
pub fn line(line: &str) -> Option<String> {
    let line = line.trim();
    (!line.is_empty() && !line.starts_with('#')).then(|| line.to_owned())
}

/// What a command file looked like when last seen: modified time and length.
type Stamp = Option<(SystemTime, u64)>;

fn stamp(path: &Path) -> Stamp {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.modified().ok()?, meta.len()))
}

/// One watched command file.
#[derive(Debug)]
pub struct Watched {
    path: PathBuf,
    seen: Stamp,
    tail: Tail,
}

impl Watched {
    /// Starts watching `path` past what it holds now.
    #[must_use]
    pub fn new(path: PathBuf) -> Self {
        Self {
            seen: stamp(&path),
            tail: Tail::at(&std::fs::read(&path).unwrap_or_default()),
            path,
        }
    }

    /// The file watched.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The lines added since the last look.
    #[must_use]
    pub fn look(&mut self) -> Vec<String> {
        let now = stamp(&self.path);
        if now == self.seen && !self.tail.waits() {
            return Vec::new();
        }
        self.seen = now;
        self.tail
            .feed(&std::fs::read(&self.path).unwrap_or_default())
    }
}

/// Watches `files` for good, handing each new line to `submit` with the file
/// it came from, and each file whose lines that ran were edited to `edited`.
pub fn watch(
    mut files: Vec<Watched>,
    mut submit: impl FnMut(&Path, String),
    mut edited: impl FnMut(&Path),
) -> ! {
    loop {
        std::thread::sleep(POLL);
        for file in &mut files {
            let lines = file.look();
            if file.tail.take_edited() {
                edited(file.path());
            }
            for line in lines {
                submit(file.path(), line);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appended_lines_run_once_and_a_partial_line_waits() {
        let mut tail = Tail::at(b"");
        assert!(tail.feed(b"enshwick sta").is_empty());
        assert_eq!(
            tail.feed(b"enshwick status\r\n# skip\n\nenshwick world\n"),
            ["enshwick status", "enshwick world"]
        );
        assert!(
            tail.feed(b"enshwick status\r\n# skip\n\nenshwick world\n")
                .is_empty()
        );
    }

    #[test]
    fn lines_present_at_start_are_skipped_but_a_partial_one_runs_when_finished() {
        let mut tail = Tail::at(b"old\nhalf");
        assert_eq!(tail.feed(b"old\nhalf done\n"), ["half done"]);
    }

    #[test]
    fn an_edited_file_skips_as_many_lines_as_already_ran_once_it_reads_the_same() {
        let mut tail = Tail::at(b"enshwick\n");
        assert!(tail.feed(b"enshwick world\nenshwick help\n").is_empty());
        assert!(tail.waits());
        assert!(!tail.take_edited(), "one read is no edit yet");
        assert_eq!(
            tail.feed(b"enshwick world\nenshwick help\n"),
            ["enshwick help"]
        );
        assert!(tail.take_edited());
        assert!(!tail.take_edited(), "an edit is reported once");
    }

    #[test]
    fn removing_a_line_that_ran_runs_no_other_line_again() {
        let mut tail = Tail::at(b"");
        assert_eq!(
            tail.feed(b"kick a\nban b\ngive c\n"),
            ["kick a", "ban b", "give c"]
        );
        assert!(tail.feed(b"ban b\ngive c\n").is_empty());
        assert!(tail.feed(b"ban b\ngive c\n").is_empty());
        assert!(tail.take_edited());
        assert_eq!(tail.feed(b"ban b\ngive c\nkick d\n"), ["kick d"]);
    }

    #[test]
    fn an_emptied_file_is_read_from_its_start() {
        let mut tail = Tail::at(b"enshwick mods\nenshwick world\n");
        assert!(tail.feed(b"enshwick help\n").is_empty());
        assert!(tail.feed(b"enshwick help\n").is_empty());
        assert!(tail.feed(b"").is_empty());
        assert!(tail.feed(b"").is_empty());
        assert_eq!(tail.feed(b"enshwick mods\n"), ["enshwick mods"]);
    }

    #[test]
    fn a_save_caught_half_written_runs_no_line_twice() {
        let mut tail = Tail::at(b"a\nb\n");
        assert!(tail.feed(b"a\n").is_empty(), "truncated mid-save");
        assert_eq!(tail.feed(b"a\nb\nc\n"), ["c"]);
        assert!(!tail.waits());
    }

    #[test]
    fn blank_lines_and_comments_are_no_console_lines() {
        assert_eq!(line("  enshwick world "), Some("enshwick world".to_owned()));
        assert_eq!(
            line("enshwick status\r"),
            Some("enshwick status".to_owned())
        );
        assert_eq!(line("# a note"), None);
        assert_eq!(line("   "), None);
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri isolates the file system")]
    fn a_watched_file_hands_out_only_lines_added_after_the_watch_began() {
        let dir = std::env::temp_dir().join(format!("enshwick-commands-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(FILE);
        std::fs::write(&path, "enshwick status\n").unwrap();
        let mut watched = Watched::new(path.clone());
        assert!(watched.look().is_empty());
        std::fs::write(&path, "enshwick status\nenshwick world\n").unwrap();
        assert_eq!(watched.look(), ["enshwick world"]);
        assert!(watched.look().is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri isolates the file system")]
    fn a_file_made_after_the_watch_began_runs_every_line() {
        let dir =
            std::env::temp_dir().join(format!("enshwick-commands-new-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(FILE);
        let mut watched = Watched::new(path.clone());
        assert!(watched.look().is_empty());
        std::fs::write(&path, "enshwick world\n").unwrap();
        assert_eq!(watched.look(), ["enshwick world"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
