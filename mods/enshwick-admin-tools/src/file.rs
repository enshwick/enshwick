//! The command file: lines appended to `mod-data/admin-tools/commands.txt`
//! run on the server's console, each once.
//!
//! The file is read again whenever its length or modified time changes.
//! Only complete lines run, so a line still being written waits for its line
//! break. Lines already in the file when the server starts are skipped; a
//! file that shrinks, or whose part already read changed (emptied or
//! replaced), is read again from its start once it reads the same twice in
//! a row, so an editor's save caught half written runs nothing twice.

/// Where the command file has been read up to.
///
/// # Examples
///
/// ```
/// use enshwick_admin_tools::file::Tail;
///
/// let mut tail = Tail::at(b"enshwick\n");
/// assert!(tail.feed(b"enshwick\n").is_empty());
/// assert_eq!(tail.feed(b"enshwick\nadmin-tools world\nadmin"), ["admin-tools world"]);
/// assert_eq!(tail.feed(b"enshwick\nadmin-tools world\nadmin-tools help\n"), ["admin-tools help"]);
/// ```
///
/// A file replaced by a longer one is read from its start, once it reads
/// the same again:
///
/// ```
/// use enshwick_admin_tools::file::Tail;
///
/// let mut tail = Tail::at(b"enshwick\n");
/// assert!(tail.feed(b"admin-tools world\nadmin-tools help\n").is_empty());
/// assert!(tail.waits());
/// assert_eq!(tail.feed(b"admin-tools world\nadmin-tools help\n"), ["admin-tools world", "admin-tools help"]);
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Tail {
    offset: usize,
    /// A hash of the part read, to see the file replaced.
    read: u64,
    /// A hash of the whole file last seen replaced or shorter, which waits
    /// for a second read before it counts.
    changed: Option<u64>,
}

/// A hash of `bytes`, the same throughout the process.
fn hash(bytes: &[u8]) -> u64 {
    use std::hash::{DefaultHasher, Hasher};
    let mut hasher = DefaultHasher::new();
    hasher.write(bytes);
    hasher.finish()
}

impl Tail {
    /// A tail past the complete lines of `content`, the file at start.
    ///
    /// # Examples
    ///
    /// See [`Tail`].
    #[must_use]
    pub fn at(content: &[u8]) -> Self {
        let offset = complete(content);
        Self {
            offset,
            read: hash(&content[..offset]),
            changed: None,
        }
    }

    /// Whether a replaced or shorter file waits for another read, even if it
    /// does not change.
    ///
    /// # Examples
    ///
    /// See [`Tail`].
    #[must_use]
    pub const fn waits(&self) -> bool {
        self.changed.is_some()
    }

    /// The console lines `content`, the whole file now, holds past the last
    /// read; moves past them.
    ///
    /// # Examples
    ///
    /// See [`Tail`].
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
            self.offset = 0;
            self.read = hash(&[]);
        }
        self.changed = None;
        let end = complete(content);
        if end <= self.offset {
            return Vec::new();
        }
        let fresh = &content[self.offset..end];
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

/// The console line a line of the file holds, trimmed; `None` for blank
/// lines and `#` comments.
///
/// # Examples
///
/// ```
/// use enshwick_admin_tools::file::line;
///
/// assert_eq!(line("  admin-tools world "), Some("admin-tools world".to_owned()));
/// assert_eq!(line("enshwick status\r"), Some("enshwick status".to_owned()));
/// assert_eq!(line("# a note"), None);
/// assert_eq!(line("   "), None);
/// ```
#[must_use]
pub fn line(line: &str) -> Option<String> {
    let line = line.trim();
    (!line.is_empty() && !line.starts_with('#')).then(|| line.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appended_lines_run_once_and_a_partial_line_waits() {
        let mut tail = Tail::at(b"");
        assert!(tail.feed(b"admin-tools wor").is_empty());
        assert_eq!(
            tail.feed(b"admin-tools world\r\n# skip\n\nadmin-tools mods\n"),
            ["admin-tools world", "admin-tools mods"]
        );
        assert!(
            tail.feed(b"admin-tools world\r\n# skip\n\nadmin-tools mods\n")
                .is_empty()
        );
    }

    #[test]
    fn lines_present_at_start_are_skipped_but_a_partial_one_runs_when_finished() {
        let mut tail = Tail::at(b"old\nhalf");
        assert_eq!(tail.feed(b"old\nhalf done\n"), ["half done"]);
    }

    #[test]
    fn a_file_that_shrinks_is_read_from_its_start() {
        let mut tail = Tail::at(b"admin-tools mods\nadmin-tools world\n");
        assert!(tail.feed(b"admin-tools help\n").is_empty());
        assert_eq!(tail.feed(b"admin-tools help\n"), ["admin-tools help"]);
        assert!(tail.feed(b"").is_empty());
        assert!(tail.feed(b"").is_empty());
        assert_eq!(tail.feed(b"admin-tools mods\n"), ["admin-tools mods"]);
    }

    #[test]
    fn a_save_caught_half_written_runs_no_line_twice() {
        let mut tail = Tail::at(b"a\nb\n");
        assert!(tail.feed(b"a\n").is_empty(), "truncated mid-save");
        assert_eq!(tail.feed(b"a\nb\nc\n"), ["c"]);
        assert!(!tail.waits());
    }
}
