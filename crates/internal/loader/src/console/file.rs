//! The command file: lines appended to `mod-data/enshwick/commands.txt` run
//! on the server's console, each once.
//!
//! The file is read again whenever its length or modified time changes.
//! Only complete lines run, so a line still being written waits for its line
//! break. Lines already in the file when the server starts are skipped, also
//! for a file that does not read then, by the length it had. A file that does
//! not read is read again on the next look and changes nothing meanwhile, so a
//! lock never runs a line again. A file that is gone and comes back goes on
//! where it was if it still starts with what ran, and runs from its start as a
//! new file otherwise.
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

    /// Whether `content` still starts with the part already read.
    #[must_use]
    pub fn continues(&self, content: &[u8]) -> bool {
        content
            .get(..self.offset)
            .is_some_and(|read| hash(read) == self.read)
    }

    /// The console lines `content`, the whole file now, holds past the last
    /// read; moves past them.
    #[must_use]
    pub fn feed(&mut self, content: &[u8]) -> Vec<String> {
        if !self.continues(content) {
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
    /// `None` until the file first reads; the lines in its first `skip`
    /// bytes then count as there at start.
    tail: Option<Tail>,
    /// The file's length at start, as its stamp gave it, for a file that did
    /// not read then.
    skip: usize,
    /// Whether the file was gone at the last look that found it.
    gone: bool,
    /// Whether the last look failed, so a failure is reported once.
    failing: bool,
}

impl Watched {
    /// Starts watching `path` past what it holds now.
    #[must_use]
    pub fn new(path: PathBuf) -> Self {
        let seen = stamp(&path);
        let tail = enshwick::files::read_if_present(&path)
            .ok()
            .map(|content| Tail::at(&content.unwrap_or_default()));
        Self {
            skip: seen.map_or(0, |(_, len)| usize::try_from(len).unwrap_or(usize::MAX)),
            seen,
            tail,
            path,
            gone: false,
            failing: false,
        }
    }

    /// The file watched.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The lines added since the last look. A file that is gone changes
    /// nothing; when it comes back, it goes on where it was if it still
    /// starts with what ran, and counts as a new file otherwise.
    ///
    /// # Errors
    ///
    /// The error of a file that is there but does not read; the next look
    /// reads it again.
    pub fn look(&mut self) -> std::io::Result<Vec<String>> {
        let now = stamp(&self.path);
        if now == self.seen && self.tail.as_ref().is_some_and(|tail| !tail.waits()) {
            return Ok(Vec::new());
        }
        let content = enshwick::files::read_if_present(&self.path)?;
        self.seen = now;
        let Some(content) = content else {
            self.gone = true;
            return Ok(Vec::new());
        };
        let tail = match &mut self.tail {
            Some(tail) => tail,
            None => self
                .tail
                .insert(Tail::at(&content[..self.skip.min(content.len())])),
        };
        if core::mem::replace(&mut self.gone, false) && !tail.continues(&content) {
            *tail = Tail::at(b"");
        }
        Ok(tail.feed(&content))
    }
}

/// Watches `files` for good, handing each new line to `submit` with its file.
///
/// A file whose lines that ran were edited goes to `edited`; a file that
/// stops reading goes to `failed`, once until it reads again.
pub fn watch(
    mut files: Vec<Watched>,
    mut submit: impl FnMut(&Path, String),
    mut edited: impl FnMut(&Path),
    mut failed: impl FnMut(&Path, &std::io::Error),
) -> ! {
    loop {
        std::thread::sleep(POLL);
        for file in &mut files {
            let lines = match file.look() {
                Ok(lines) => {
                    file.failing = false;
                    lines
                }
                Err(error) => {
                    if !core::mem::replace(&mut file.failing, true) {
                        failed(file.path(), &error);
                    }
                    continue;
                }
            };
            if file.tail.as_mut().is_some_and(Tail::take_edited) {
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
        assert!(watched.look().unwrap().is_empty());
        std::fs::write(&path, "enshwick status\nenshwick world\n").unwrap();
        assert_eq!(watched.look().unwrap(), ["enshwick world"]);
        assert!(watched.look().unwrap().is_empty());
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
        assert!(watched.look().unwrap().is_empty());
        std::fs::write(&path, "enshwick world\n").unwrap();
        assert_eq!(watched.look().unwrap(), ["enshwick world"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A fresh folder for one test, named `name`.
    fn dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("enshwick-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri isolates the file system")]
    fn a_read_error_runs_no_line_again() {
        let dir = dir("commands-locked");
        let path = dir.join(FILE);
        let mut watched = Watched::new(path.clone());
        std::fs::write(&path, "give 7 1 Ann\nban Bob\n").unwrap();
        assert_eq!(watched.look().unwrap(), ["give 7 1 Ann", "ban Bob"]);
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        assert!(watched.look().is_err(), "a folder does not read as a file");
        assert!(watched.look().is_err());
        std::fs::remove_dir(&path).unwrap();
        std::fs::write(&path, "give 7 1 Ann\nban Bob\n").unwrap();
        assert!(watched.look().unwrap().is_empty());
        std::fs::write(&path, "give 7 1 Ann\nban Bob\nkick Cy\n").unwrap();
        assert_eq!(watched.look().unwrap(), ["kick Cy"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri isolates the file system")]
    fn a_file_gone_for_a_while_runs_no_line_again() {
        let dir = dir("commands-gone");
        let path = dir.join(FILE);
        let mut watched = Watched::new(path.clone());
        std::fs::write(&path, "give 7 1 Ann\n").unwrap();
        assert_eq!(watched.look().unwrap(), ["give 7 1 Ann"]);
        std::fs::remove_file(&path).unwrap();
        assert!(watched.look().unwrap().is_empty());
        assert!(watched.look().unwrap().is_empty());
        std::fs::write(&path, "give 7 1 Ann\n").unwrap();
        assert!(watched.look().unwrap().is_empty());
        assert!(watched.look().unwrap().is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    #[cfg(windows)]
    #[cfg_attr(miri, ignore = "Miri isolates the file system")]
    fn a_file_locked_at_start_skips_only_the_lines_it_held_then() {
        use std::io::Write as _;
        use std::os::windows::fs::OpenOptionsExt as _;
        let dir = dir("commands-start");
        let path = dir.join(FILE);
        std::fs::write(&path, "give 7 1 Ann\n").unwrap();
        // A writer that shares the file with no one, as some panels do.
        let mut writer = std::fs::File::options()
            .append(true)
            .share_mode(0)
            .open(&path)
            .unwrap();
        let mut watched = Watched::new(path.clone());
        assert!(watched.look().is_err());
        writer.write_all(b"kick Cy\n").unwrap();
        drop(writer);
        assert_eq!(watched.look().unwrap(), ["kick Cy"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri isolates the file system")]
    fn a_file_made_anew_after_it_was_gone_runs_from_its_start() {
        let dir = dir("commands-rotated");
        let path = dir.join(FILE);
        let mut watched = Watched::new(path.clone());
        std::fs::write(&path, "give 7 1 Ann\nban Bob\n").unwrap();
        assert_eq!(watched.look().unwrap(), ["give 7 1 Ann", "ban Bob"]);
        std::fs::remove_file(&path).unwrap();
        assert!(watched.look().unwrap().is_empty());
        std::fs::write(&path, "kick Cy\n").unwrap();
        assert_eq!(watched.look().unwrap(), ["kick Cy"]);
        std::fs::write(&path, "kick Cy\nkick Di\n").unwrap();
        assert_eq!(watched.look().unwrap(), ["kick Di"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
