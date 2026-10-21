//! The command file: lines appended to `mod-data/admin-tools/commands.txt`
//! run as the server console's commands, each once.
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
/// let mut tail = Tail::at(b"/admin mods\n");
/// assert!(tail.feed(b"/admin mods\n").is_empty());
/// assert_eq!(tail.feed(b"/admin mods\n/admin world\n/admin"), ["/admin world"]);
/// assert_eq!(tail.feed(b"/admin mods\n/admin world\n/admin help\n"), ["/admin help"]);
/// ```
///
/// A file replaced by a longer one is read from its start, once it reads
/// the same again:
///
/// ```
/// use enshwick_admin_tools::file::Tail;
///
/// let mut tail = Tail::at(b"/admin mods\n");
/// assert!(tail.feed(b"/admin world\n/admin help\n").is_empty());
/// assert!(tail.waits());
/// assert_eq!(tail.feed(b"/admin world\n/admin help\n"), ["/admin world", "/admin help"]);
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

    /// The command lines `content`, the whole file now, holds past the last
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
            .filter_map(command)
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

/// The command a line of the file asks for: trimmed, with a `/` added when
/// it has none; `None` for blank lines and `#` comments.
///
/// # Examples
///
/// ```
/// use enshwick_admin_tools::file::command;
///
/// assert_eq!(command("  admin world "), Some("/admin world".to_owned()));
/// assert_eq!(command("/enshwick status\r"), Some("/enshwick status".to_owned()));
/// assert_eq!(command("# a note"), None);
/// assert_eq!(command("   "), None);
/// ```
#[must_use]
pub fn command(line: &str) -> Option<String> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    Some(if line.starts_with('/') {
        line.to_owned()
    } else {
        format!("/{line}")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appended_lines_run_once_and_a_partial_line_waits() {
        let mut tail = Tail::at(b"");
        assert!(tail.feed(b"/admin wor").is_empty());
        assert_eq!(
            tail.feed(b"/admin world\r\n# skip\n\nadmin mods\n"),
            ["/admin world", "/admin mods"]
        );
        assert!(
            tail.feed(b"/admin world\r\n# skip\n\nadmin mods\n")
                .is_empty()
        );
    }

    #[test]
    fn lines_present_at_start_are_skipped_but_a_partial_one_runs_when_finished() {
        let mut tail = Tail::at(b"/old\n/half");
        assert_eq!(tail.feed(b"/old\n/half done\n"), ["/half done"]);
    }

    #[test]
    fn a_file_that_shrinks_is_read_from_its_start() {
        let mut tail = Tail::at(b"/admin mods\n/admin world\n");
        assert!(tail.feed(b"/admin help\n").is_empty());
        assert_eq!(tail.feed(b"/admin help\n"), ["/admin help"]);
        assert!(tail.feed(b"").is_empty());
        assert!(tail.feed(b"").is_empty());
        assert_eq!(tail.feed(b"/admin mods\n"), ["/admin mods"]);
    }

    #[test]
    fn a_save_caught_half_written_runs_no_line_twice() {
        let mut tail = Tail::at(b"a\nb\n");
        assert!(tail.feed(b"a\n").is_empty(), "truncated mid-save");
        assert_eq!(tail.feed(b"a\nb\nc\n"), ["/c"]);
        assert!(!tail.waits());
    }
}
