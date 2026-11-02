//! The command file: lines appended to `mod-data/enshwick/commands.txt` run
//! on the server's console, each once.
//!
//! The file is read again whenever its length or modified time changes.
//! Only complete lines run, so a line still being written waits for its line
//! break. Lines already in the file when the server starts are skipped, also
//! for a file that does not read then, by the length it had. A file that does
//! not read is read again on the next look and changes nothing meanwhile, so a
//! lock never runs a line again. A file that is gone and comes back, or
//! another file put in its place, goes on where it was if it still starts with
//! what ran; one that holds none of the lines that ran is a new file and runs
//! from its start once it reads the same twice; any other counts as an edit.
//!
//! The file is append-only. A file whose part already read changed counts
//! once it reads the same twice in a row, so an editor's save caught half
//! written runs nothing. An emptied file is read again from its start; any
//! other edit skips as many lines as already ran, so a kick, ban or give
//! never runs twice, and the edit is reported for a warning.

use std::hash::{DefaultHasher, Hasher};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

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

/// What a look at a command file has for the log besides its lines.
#[derive(Debug)]
pub enum Note {
    /// The file is there but does not read; `quiet` more failures since the
    /// last such note went unreported.
    Unread {
        /// Why.
        error: std::io::Error,
        /// Failures not reported, at most one note per [`WARN_EVERY`].
        quiet: u32,
    },
    /// The file reads again after an [`Note::Unread`]; `skipped` lines it
    /// held at start, when it did not read, do not run.
    ReadsAgain {
        /// Complete lines skipped as there at start.
        skipped: usize,
    },
    /// Lines that ran were changed or removed, so only lines past their
    /// count run.
    Edited,
}

/// At most one [`Note::Unread`] per this long for one file.
const WARN_EVERY: Duration = Duration::from_secs(60);

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
    /// Whether the file was gone, or another file came in its place, and was
    /// not taken up again yet.
    gone: bool,
    /// Which file was read last.
    identity: Identity,
    /// A hash of the whole file that came back without the lines that ran,
    /// which waits for a second read before it counts as new.
    anew: Option<u64>,
    /// Hashes of every console line that ran or was there at start, so a
    /// file that comes back holding one of them is never taken as new.
    ran: rustc_hash::FxHashSet<u64>,
    /// The error of a file that did not read at start, for the first look.
    unread_at_start: Option<std::io::Error>,
    /// Whether the file fails to read, and whether that was noted.
    failing: Option<bool>,
    /// When a [`Note::Unread`] was last given.
    noted: Option<Instant>,
    /// Failures since then that were not noted.
    quiet: u32,
}

impl Watched {
    /// Starts watching `path`, skipping the lines it holds now; for a file
    /// that does not read now, the lines in as many bytes as it is long.
    #[must_use]
    pub fn new(path: PathBuf) -> Self {
        let seen = stamp(&path);
        let (content, identity, unread_at_start) = match read_with_identity(&path) {
            Ok(Some((content, identity))) => (Some(content), identity, None),
            Ok(None) => (Some(Vec::new()), None, None),
            Err(error) => (None, None, Some(error)),
        };
        let tail = content.as_deref().map(Tail::at);
        let ran = content
            .as_deref()
            .map(|content| lines_of(&content[..complete(content)]))
            .unwrap_or_default()
            .iter()
            .map(|line| hash(line.as_bytes()))
            .collect();
        Self {
            skip: seen.map_or(0, |(_, len)| usize::try_from(len).unwrap_or(usize::MAX)),
            seen,
            tail,
            path,
            gone: false,
            identity,
            anew: None,
            unread_at_start,
            ran,
            failing: None,
            noted: None,
            quiet: 0,
        }
    }

    /// The file watched.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The lines added since the last look, and what to log about the file.
    pub fn look(&mut self) -> (Vec<String>, Vec<Note>) {
        let mut notes = Vec::new();
        let lines = match self.fresh_lines() {
            Ok((lines, skipped)) => {
                if self.failing.take() == Some(true) && !self.gone {
                    notes.push(Note::ReadsAgain { skipped });
                }
                if self.tail.as_mut().is_some_and(Tail::take_edited) {
                    notes.push(Note::Edited);
                }
                lines
            }
            Err(error) => {
                let now = Instant::now();
                let due = self
                    .noted
                    .is_none_or(|at| now.duration_since(at) >= WARN_EVERY);
                if due {
                    self.noted = Some(now);
                    notes.push(Note::Unread {
                        error,
                        quiet: core::mem::take(&mut self.quiet),
                    });
                } else {
                    self.quiet = self.quiet.saturating_add(1);
                }
                self.failing = Some(self.failing.unwrap_or(false) || due);
                Vec::new()
            }
        };
        (lines, notes)
    }

    /// The lines added since the last look and how many lines held at start
    /// a first read skipped. A file that is gone changes nothing. A file that
    /// comes back, or another file put in its place, goes on where it was if
    /// it still starts with what ran; one that holds no line that ran counts
    /// as a new file once it reads the same twice in a row, and any other is
    /// an edit, which skips as many lines as ran.
    fn fresh_lines(&mut self) -> std::io::Result<(Vec<String>, usize)> {
        if let Some(error) = self.unread_at_start.take() {
            return Err(error);
        }
        let now = stamp(&self.path);
        if now == self.seen && self.tail.as_ref().is_some_and(|tail| !tail.waits()) {
            return Ok((Vec::new(), 0));
        }
        let read = read_with_identity(&self.path)?;
        self.seen = now;
        let Some((content, identity)) = read else {
            self.gone = true;
            // A file gone before it ever read is new when it comes.
            self.tail.get_or_insert_with(|| Tail::at(b""));
            return Ok((Vec::new(), 0));
        };
        let replaced = identity.is_some() && self.identity.is_some() && identity != self.identity;
        if identity.is_some() {
            self.identity = identity;
        }
        let mut skipped = 0;
        if self.tail.is_none() {
            let start = &content[..complete(&content[..self.skip.min(content.len())])];
            skipped = self.note_ran(start);
            self.tail = Some(Tail::at(start));
        }
        let Some(tail) = &mut self.tail else {
            return Ok((Vec::new(), skipped));
        };
        // A file with no line that ran or was there at start is new; any
        // other goes through the edit check, so no line runs twice.
        let new = (self.gone || replaced)
            && !tail.continues(&content)
            && !lines_of(&content[..complete(&content)])
                .iter()
                .any(|line| self.ran.contains(&hash(line.as_bytes())));
        if new {
            let whole = hash(&content);
            if self.anew != Some(whole) {
                self.anew = Some(whole);
                self.gone = true;
                // Read again at the next look, even if the file stays as it is.
                self.seen = None;
                return Ok((Vec::new(), skipped));
            }
            *tail = Tail::at(b"");
        }
        self.gone = false;
        self.anew = None;
        let fresh = tail.feed(&content);
        self.ran
            .extend(fresh.iter().map(|line| hash(line.as_bytes())));
        Ok((fresh, skipped))
    }

    /// Notes the console lines of `content` as there at start; how many.
    fn note_ran(&mut self, content: &[u8]) -> usize {
        let lines = lines_of(content);
        self.ran
            .extend(lines.iter().map(|line| hash(line.as_bytes())));
        lines.len()
    }
}

/// The console lines of `content`.
fn lines_of(content: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(content)
        .lines()
        .filter_map(line)
        .collect()
}

/// The file's volume and index, which tell another file put at its path.
type Identity = Option<(u32, u64)>;

/// What `path` holds and which file it is; `None` for a file that is not
/// there.
fn read_with_identity(path: &Path) -> std::io::Result<Option<(Vec<u8>, Identity)>> {
    use std::io::Read as _;
    let mut file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    let identity = identity(&file);
    let mut content = Vec::new();
    file.read_to_end(&mut content)?;
    Ok(Some((content, identity)))
}

#[cfg(windows)]
fn identity(file: &std::fs::File) -> Identity {
    use std::os::windows::io::AsRawHandle as _;
    use windows_sys::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
    };
    let mut info = core::mem::MaybeUninit::<BY_HANDLE_FILE_INFORMATION>::uninit();
    // SAFETY: `file` keeps its handle open across the call, and `info` is a
    // local the call fills whole when it returns non-zero.
    let filled = unsafe { GetFileInformationByHandle(file.as_raw_handle(), info.as_mut_ptr()) };
    if filled == 0 {
        return None;
    }
    // SAFETY: the call returned non-zero, so it filled `info`.
    let info = unsafe { info.assume_init() };
    let index = (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow);
    Some((info.dwVolumeSerialNumber, index))
}

#[cfg(not(windows))]
fn identity(_: &std::fs::File) -> Identity {
    None
}

/// Watches `files` for good, handing each new line to `submit` and each
/// [`Note`] to `report`, with the file they came from.
pub fn watch(
    mut files: Vec<Watched>,
    mut submit: impl FnMut(&Path, String),
    mut report: impl FnMut(&Path, Note),
) -> ! {
    loop {
        std::thread::sleep(POLL);
        for file in &mut files {
            let (lines, notes) = file.look();
            for note in notes {
                report(file.path(), note);
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

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("enshwick-commands-{name}-{}", std::process::id()));
        let _gone = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// The lines of the next look at `watched`, which must note nothing.
    fn lines(watched: &mut Watched) -> Vec<String> {
        let (lines, notes) = watched.look();
        assert!(notes.is_empty(), "{notes:?}");
        lines
    }

    /// The notes of the next look at `watched`, which must hand out no line.
    fn notes(watched: &mut Watched) -> Vec<Note> {
        let (lines, notes) = watched.look();
        assert!(lines.is_empty(), "{lines:?}");
        notes
    }

    /// A writer of `path` that shares it with no one, as some panels and
    /// editors do, so it does not read while the writer is open.
    #[cfg(windows)]
    fn locked(path: &Path) -> std::fs::File {
        use std::os::windows::fs::OpenOptionsExt as _;
        std::fs::File::options()
            .create(true)
            .append(true)
            .share_mode(0)
            .open(path)
            .unwrap()
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri isolates the file system")]
    fn a_watched_file_hands_out_only_lines_added_after_the_watch_began() {
        let dir = scratch("added");
        let path = dir.join(FILE);
        std::fs::write(&path, "enshwick status\n").unwrap();
        let mut watched = Watched::new(path.clone());
        assert!(lines(&mut watched).is_empty());
        std::fs::write(&path, "enshwick status\nenshwick world\n").unwrap();
        assert_eq!(lines(&mut watched), ["enshwick world"]);
        assert!(lines(&mut watched).is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri isolates the file system")]
    fn a_file_made_after_the_watch_began_runs_every_line() {
        let dir = scratch("new");
        let path = dir.join(FILE);
        let mut watched = Watched::new(path.clone());
        assert!(lines(&mut watched).is_empty());
        std::fs::write(&path, "enshwick world\n").unwrap();
        assert_eq!(lines(&mut watched), ["enshwick world"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri isolates the file system")]
    fn a_read_error_runs_no_line_again_and_is_noted_once_until_it_reads() {
        let dir = scratch("locked");
        let path = dir.join(FILE);
        let mut watched = Watched::new(path.clone());
        std::fs::write(&path, "give 7 1 Ann\nban Bob\n").unwrap();
        assert_eq!(lines(&mut watched), ["give 7 1 Ann", "ban Bob"]);
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        assert!(matches!(
            notes(&mut watched)[..],
            [Note::Unread { quiet: 0, .. }]
        ));
        assert!(
            notes(&mut watched).is_empty(),
            "noted at most once a minute"
        );
        std::fs::remove_dir(&path).unwrap();
        std::fs::write(&path, "give 7 1 Ann\nban Bob\n").unwrap();
        assert!(matches!(
            notes(&mut watched)[..],
            [Note::ReadsAgain { skipped: 0 }]
        ));
        std::fs::write(&path, "give 7 1 Ann\nban Bob\nkick Cy\n").unwrap();
        assert_eq!(lines(&mut watched), ["kick Cy"]);
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        assert!(
            notes(&mut watched).is_empty(),
            "a failure soon after is counted"
        );
        std::fs::remove_dir(&path).unwrap();
        std::fs::write(&path, "give 7 1 Ann\nban Bob\nkick Cy\n").unwrap();
        assert!(lines(&mut watched).is_empty(), "nor does its end get noted");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri isolates the file system")]
    fn a_file_gone_for_a_while_runs_no_line_again() {
        let dir = scratch("gone");
        let path = dir.join(FILE);
        let mut watched = Watched::new(path.clone());
        std::fs::write(&path, "give 7 1 Ann\n").unwrap();
        assert_eq!(lines(&mut watched), ["give 7 1 Ann"]);
        std::fs::remove_file(&path).unwrap();
        assert!(lines(&mut watched).is_empty());
        assert!(lines(&mut watched).is_empty());
        std::fs::write(&path, "give 7 1 Ann\n").unwrap();
        assert!(lines(&mut watched).is_empty());
        assert!(lines(&mut watched).is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri isolates the file system")]
    fn a_file_made_anew_after_it_was_gone_runs_from_its_start() {
        let dir = scratch("rotated");
        let path = dir.join(FILE);
        let mut watched = Watched::new(path.clone());
        std::fs::write(&path, "give 7 1 Ann\nban Bob\n").unwrap();
        assert_eq!(lines(&mut watched), ["give 7 1 Ann", "ban Bob"]);
        std::fs::remove_file(&path).unwrap();
        assert!(lines(&mut watched).is_empty());
        std::fs::write(&path, "kick Cy\n").unwrap();
        assert!(
            lines(&mut watched).is_empty(),
            "read twice before it counts"
        );
        assert_eq!(lines(&mut watched), ["kick Cy"]);
        std::fs::write(&path, "kick Cy\nkick Di\n").unwrap();
        assert_eq!(lines(&mut watched), ["kick Di"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    #[cfg(windows)]
    #[cfg_attr(miri, ignore = "Miri isolates the file system")]
    fn another_file_put_in_its_place_between_looks_runs_from_its_start() {
        let dir = scratch("renamed");
        let path = dir.join(FILE);
        let mut watched = Watched::new(path.clone());
        std::fs::write(&path, "give 7 1 Ann\nban Bob\n").unwrap();
        assert_eq!(lines(&mut watched), ["give 7 1 Ann", "ban Bob"]);
        let staged = dir.join("commands.tmp");
        std::fs::write(&staged, "kick Cy\n").unwrap();
        std::fs::rename(&staged, &path).unwrap();
        assert!(
            lines(&mut watched).is_empty(),
            "read twice before it counts"
        );
        assert_eq!(lines(&mut watched), ["kick Cy"]);
        std::fs::write(&staged, "kick Cy\nkick Di\n").unwrap();
        std::fs::rename(&staged, &path).unwrap();
        assert_eq!(
            lines(&mut watched),
            ["kick Di"],
            "a rename that appends goes on"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri isolates the file system")]
    fn a_file_coming_back_half_written_runs_no_line_again() {
        let dir = scratch("restored");
        let path = dir.join(FILE);
        let mut watched = Watched::new(path.clone());
        std::fs::write(&path, "give 7 1 Ann\nban Bob\n").unwrap();
        assert_eq!(lines(&mut watched), ["give 7 1 Ann", "ban Bob"]);
        std::fs::remove_file(&path).unwrap();
        assert!(lines(&mut watched).is_empty());
        std::fs::write(&path, "give 7 1 Ann\n").unwrap();
        assert!(lines(&mut watched).is_empty(), "caught half restored");
        std::fs::write(&path, "give 7 1 Ann\nban Bob\nkick Cy\n").unwrap();
        assert_eq!(lines(&mut watched), ["kick Cy"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri isolates the file system")]
    fn an_edit_saved_through_a_rename_runs_no_line_again() {
        let dir = scratch("rename-edit");
        let path = dir.join(FILE);
        let mut watched = Watched::new(path.clone());
        std::fs::write(&path, "give 7 1 Ann\nban Bob\n").unwrap();
        assert_eq!(lines(&mut watched), ["give 7 1 Ann", "ban Bob"]);
        let staged = dir.join("commands.tmp");
        std::fs::write(&staged, "ban Bob\n").unwrap();
        std::fs::rename(&staged, &path).unwrap();
        for _ in 0..3 {
            assert!(watched.look().0.is_empty());
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri isolates the file system")]
    fn an_older_copy_put_back_runs_no_line_again() {
        let dir = scratch("older-copy");
        let path = dir.join(FILE);
        let mut watched = Watched::new(path.clone());
        std::fs::write(&path, "give 7 1 Ann\nban Bob\nkick Cy\n").unwrap();
        assert_eq!(lines(&mut watched), ["give 7 1 Ann", "ban Bob", "kick Cy"]);
        std::fs::remove_file(&path).unwrap();
        assert!(lines(&mut watched).is_empty());
        std::fs::write(&path, "give 7 1 Ann\nban Bob\n").unwrap();
        for _ in 0..3 {
            assert!(watched.look().0.is_empty());
        }
        std::fs::write(&path, "give 7 1 Ann\nban Bob\nkick Di\n").unwrap();
        assert_eq!(
            watched.look().0,
            ["kick Di"],
            "past the count kept by the edit"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    #[cfg(windows)]
    #[cfg_attr(miri, ignore = "Miri isolates the file system")]
    fn a_file_locked_at_start_skips_only_the_lines_it_held_then_and_says_so() {
        use std::io::Write as _;
        let dir = scratch("start");
        let path = dir.join(FILE);
        std::fs::write(&path, "give 7 1 Ann\n").unwrap();
        let mut writer = locked(&path);
        let mut watched = Watched::new(path.clone());
        assert!(matches!(notes(&mut watched)[..], [Note::Unread { .. }]));
        writer.write_all(b"kick Cy\n").unwrap();
        drop(writer);
        let (lines, notes) = watched.look();
        assert_eq!(lines, ["kick Cy"]);
        assert!(
            matches!(notes[..], [Note::ReadsAgain { skipped: 1 }]),
            "{notes:?}"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    #[cfg(windows)]
    #[cfg_attr(miri, ignore = "Miri isolates the file system")]
    fn a_file_locked_at_start_then_deleted_runs_every_line_of_the_next_one() {
        let dir = scratch("start-gone");
        let path = dir.join(FILE);
        std::fs::write(&path, "give 7 1 Ann\nban Bob\nkick Cy\n").unwrap();
        let writer = locked(&path);
        let mut watched = Watched::new(path.clone());
        drop(writer);
        std::fs::remove_file(&path).unwrap();
        assert!(matches!(notes(&mut watched)[..], [Note::Unread { .. }]));
        assert!(
            notes(&mut watched).is_empty(),
            "a gone file does not read again"
        );
        std::fs::write(&path, "kick Di\n").unwrap();
        assert_eq!(lines(&mut watched), ["kick Di"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
