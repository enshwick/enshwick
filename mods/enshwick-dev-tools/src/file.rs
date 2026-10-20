//! The command file: lines appended to `mod-data/dev-tools/commands.txt`
//! run as the server console's commands, each once.
//!
//! The file is read again whenever its length or modified time changes.
//! Only complete lines run, so a line still being written waits for its line
//! break. Lines already in the file when the server starts are skipped; a
//! file that shrinks, emptied or replaced, is read from its start again.

/// Where the command file has been read up to.
///
/// # Examples
///
/// ```
/// use enshwick_dev_tools::file::Tail;
///
/// let mut tail = Tail::at(b"/dev mods\n");
/// assert!(tail.feed(b"/dev mods\n").is_empty());
/// assert_eq!(tail.feed(b"/dev mods\n/dev world\n/dev"), ["/dev world"]);
/// assert_eq!(tail.feed(b"/dev mods\n/dev world\n/dev help\n"), ["/dev help"]);
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Tail {
    offset: usize,
}

impl Tail {
    /// A tail past the complete lines of `content`, the file at start.
    ///
    /// # Examples
    ///
    /// See [`Tail`].
    #[must_use]
    pub fn at(content: &[u8]) -> Self {
        Self {
            offset: complete(content),
        }
    }

    /// The command lines `content`, the whole file now, holds past the last
    /// read; moves past them.
    ///
    /// # Examples
    ///
    /// See [`Tail`].
    #[must_use]
    pub fn feed(&mut self, content: &[u8]) -> Vec<String> {
        if content.len() < self.offset {
            self.offset = 0;
        }
        let end = complete(content);
        if end <= self.offset {
            return Vec::new();
        }
        let fresh = &content[self.offset..end];
        self.offset = end;
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
/// use enshwick_dev_tools::file::command;
///
/// assert_eq!(command("  dev world "), Some("/dev world".to_owned()));
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
        assert!(tail.feed(b"/dev wor").is_empty());
        assert_eq!(
            tail.feed(b"/dev world\r\n# skip\n\ndev mods\n"),
            ["/dev world", "/dev mods"]
        );
        assert!(tail.feed(b"/dev world\r\n# skip\n\ndev mods\n").is_empty());
    }

    #[test]
    fn lines_present_at_start_are_skipped_but_a_partial_one_runs_when_finished() {
        let mut tail = Tail::at(b"/old\n/half");
        assert_eq!(tail.feed(b"/old\n/half done\n"), ["/half done"]);
    }

    #[test]
    fn a_file_that_shrinks_is_read_from_its_start() {
        let mut tail = Tail::at(b"/dev mods\n/dev world\n");
        assert_eq!(tail.feed(b"/dev help\n"), ["/dev help"]);
        assert!(tail.feed(b"").is_empty());
        assert_eq!(tail.feed(b"/dev mods\n"), ["/dev mods"]);
    }
}
