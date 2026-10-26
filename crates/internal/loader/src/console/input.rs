//! The dedicated server's own console window or terminal: the game reads
//! nothing from its standard input, so the loader reads lines there.
//!
//! A server with no input (a service, Docker without `-i`, a closed pipe)
//! reads end of file at once, and the reader ends without a word; reading
//! never blocks the game, since it runs on a thread of its own.

use std::io::BufRead;

/// The longest line read, in bytes; the rest of a longer one is dropped.
const LONGEST: usize = 4096;

/// Why the reader stopped.
#[derive(Debug, PartialEq, Eq)]
pub enum End {
    /// The input ended or there was none.
    Closed,
    /// The input broke, with what the system said.
    Failed(String),
}

/// Reads `input` line by line until it ends, handing every console line to
/// `submit`; why it stopped.
pub fn read(mut input: impl BufRead, mut submit: impl FnMut(String)) -> End {
    let mut buffer = Vec::new();
    loop {
        buffer.clear();
        match input.read_until(b'\n', &mut buffer) {
            Ok(0) => return End::Closed,
            Ok(_) => {
                buffer.truncate(LONGEST);
                if let Some(line) = super::file::line(&String::from_utf8_lossy(&buffer)) {
                    submit(line);
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return End::Failed(e.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_line_typed_reaches_the_console_until_the_input_ends() {
        let mut lines = Vec::new();
        let end = read(
            &b"enshwick status\r\n\n# note\n  enshwick world  \nenshwick help"[..],
            |line| lines.push(line),
        );
        assert_eq!(end, End::Closed);
        assert_eq!(
            lines,
            ["enshwick status", "enshwick world", "enshwick help"]
        );
    }

    #[test]
    fn no_input_ends_the_reader_at_once() {
        let mut lines = Vec::new();
        assert_eq!(read(std::io::empty(), |line| lines.push(line)), End::Closed);
        assert!(lines.is_empty());
    }

    #[test]
    fn a_line_past_the_longest_is_cut() {
        let long = format!("enshwick {}\n", "x".repeat(LONGEST * 2));
        let mut lines = Vec::new();
        let _end = read(long.as_bytes(), |line| lines.push(line));
        assert_eq!(lines.len(), 1);
        assert!(lines[0].len() <= LONGEST);
    }

    #[test]
    fn a_broken_input_ends_the_reader_with_its_reason() {
        struct Broken;
        impl std::io::Read for Broken {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("gone"))
            }
        }
        let end = read(std::io::BufReader::new(Broken), |_| {});
        assert_eq!(end, End::Failed("gone".to_owned()));
    }
}
