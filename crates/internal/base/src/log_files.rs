//! The loader's log files, how their lines read and the start lines tools
//! wait for in them: the one definition the loader writes by and
//! `cargo enshwick`, which does not link the loader, reads by.

use core::fmt::{Display, Write as _};
use core::ops::Range;

use crate::abi::Level;

/// The folder beside the game's executable that holds every log.
pub const LOGS: &str = "logs";

/// The loader's settings file in the game's folder, which sets the levels
/// the log files keep.
pub const SETTINGS_FILE: &str = "enshwick.toml";

/// The key of [`SETTINGS_FILE`] naming the most detailed level kept.
pub const LOG_LEVEL_KEY: &str = "log_level";

/// The key of [`SETTINGS_FILE`] naming the same per source, over
/// [`LOG_LEVEL_KEY`].
pub const LOG_LEVELS_KEY: &str = "log_levels";

/// The length of a run file's stamp, `2026-11-03_10-00-00`, UTC.
pub const STAMP_LEN: usize = 19;

/// The name of `source`'s file of the run stamped `stamp`:
/// `<source>_<stamp>.log`.
///
/// # Examples
///
/// ```
/// use enshwick_base::log_files::run_file_name;
///
/// assert_eq!(run_file_name("enshwick", "2026-11-03_10-00-00"), "enshwick_2026-11-03_10-00-00.log");
/// ```
#[must_use]
pub fn run_file_name(source: &str, stamp: &str) -> String {
    format!("{source}_{stamp}.log")
}

/// The source and stamp of a run file's `name`; `None` for any other file.
///
/// # Examples
///
/// ```
/// use enshwick_base::log_files::run_file;
///
/// assert_eq!(
///     run_file("you.my-mod_2026-11-03_10-00-00.log"),
///     Some(("you.my-mod", "2026-11-03_10-00-00"))
/// );
/// assert_eq!(run_file("start.log"), None);
/// ```
#[must_use]
pub fn run_file(name: &str) -> Option<(&str, &str)> {
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

/// Appends the line `HH:MM:SS.mmmZ <level> [<source>] <message>` and its
/// newline to `out`, at `millis` since the Unix epoch, and gives where the
/// source sits in `out`.
///
/// # Examples
///
/// ```
/// use enshwick_base::abi::Level;
/// use enshwick_base::log_files::write_line;
///
/// let mut out = String::new();
/// let source = write_line(&mut out, 449, Level::INFO, "enshwick", "hi");
/// assert_eq!(out, "00:00:00.449Z info  [enshwick] hi\n");
/// assert_eq!(&out[source], "enshwick");
/// ```
pub fn write_line(
    out: &mut String,
    millis: u64,
    level: Level,
    source: &str,
    message: &str,
) -> Range<usize> {
    let secs = millis / 1000;
    let (h, m, s) = (secs / 3600 % 24, secs / 60 % 60, secs % 60);
    let name = level.name();
    let _time = write!(
        out,
        "{h:02}:{m:02}:{s:02}.{:03}Z {name:<5} [",
        millis % 1000
    );
    let start = out.len();
    out.push_str(source);
    let at = start..out.len();
    let _message = writeln!(out, "] {message}");
    at
}

/// A line [`write_line`] wrote, read back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct Line<'a> {
    /// The milliseconds into the day, UTC.
    pub millis: u32,
    /// The level the line was written at.
    pub level: Level,
    /// The loader's id or a mod's.
    pub source: &'a str,
    /// The text after the source, without its continuation lines.
    pub message: &'a str,
}

impl<'a> Line<'a> {
    /// The line `text` opens, without its newline; `None` for one that
    /// continues the line before, or any other text.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_base::abi::Level;
    /// use enshwick_base::log_files::Line;
    ///
    /// let line = Line::parse("00:00:01.000Z warn  [you.mod] slow");
    /// assert_eq!(line.map(|l| (l.millis, l.level, l.source)), Some((1000, Level::WARN, "you.mod")));
    /// assert_eq!(Line::parse("  continued"), None);
    /// ```
    #[must_use]
    pub fn parse(text: &'a str) -> Option<Self> {
        let (time, rest) = text.split_once(' ')?;
        let millis = time_of_day(time)?;
        let (name, rest) = rest.trim_start().split_once(' ')?;
        let level = name.parse().ok()?;
        let (source, message) = rest.trim_start().strip_prefix('[')?.split_once("] ")?;
        Some(Self {
            millis,
            level,
            source,
            message,
        })
    }
}

/// The milliseconds into the day of a line's time, `HH:MM:SS.mmmZ`.
fn time_of_day(time: &str) -> Option<u32> {
    let (clock, millis) = time.strip_suffix('Z')?.split_once('.')?;
    if clock.len() != 8 || millis.len() != 3 {
        return None;
    }
    let mut parts = clock.splitn(3, ':').map(|part| part.parse::<u32>().ok());
    let (hours, minutes, seconds) = (parts.next()??, parts.next()??, parts.next()??);
    Some(((hours * 60 + minutes) * 60 + seconds) * 1000 + millis.parse::<u32>().ok()?)
}

/// How the loader's line opens once every mod's `ready` ran.
pub const READY: &str = "ready: ";

/// The loader's line once every mod's `ready` ran.
///
/// # Examples
///
/// ```
/// use enshwick_base::log_files::ready;
///
/// assert_eq!(ready(2, 3), "ready: 2 of 3 mods running");
/// ```
#[must_use]
pub fn ready(running: usize, all: usize) -> String {
    format!("{READY}{running} of {all} mods running")
}

/// How the loader's line opens as a world loads, with its key or without
/// one, before its mods read their settings.
pub const WORLD_SETTINGS: &str = "world settings: ";

/// The loader's line as a world without a key yet loads.
pub const NO_WORLD_KEY: &str = const_format::concatcp!(
    WORLD_SETTINGS,
    "the world has no key yet; mods run on their defaults"
);

/// The loader's line as the world `key` loads.
///
/// # Examples
///
/// ```
/// use enshwick_base::log_files::world_settings;
///
/// assert_eq!(world_settings("D:/saves/world0"), "world settings: world D:/saves/world0");
/// ```
#[must_use]
pub fn world_settings(key: &str) -> String {
    format!("{WORLD_SETTINGS}world {key}")
}

/// How the loader's line ends for a mod whose `init` passed.
const INITIALISED: &str = " initialised";

/// The loader's line for the mod `id` at `version` whose `init` passed.
///
/// # Examples
///
/// ```
/// use enshwick_base::log_files::initialised;
///
/// assert_eq!(initialised("you.hello", "0.1.0"), "you.hello 0.1.0 initialised");
/// ```
#[must_use]
pub fn initialised(id: &str, version: &str) -> String {
    format!("{id} {version}{INITIALISED}")
}

/// The id and version an [`initialised`] line names; `None` for any other
/// line.
///
/// # Examples
///
/// ```
/// use enshwick_base::log_files::initialised_mod;
///
/// assert_eq!(initialised_mod("you.hello 0.1.0 initialised"), Some(("you.hello", "0.1.0")));
/// assert_eq!(initialised_mod("ready: 1 of 1 mods running"), None);
/// ```
#[must_use]
pub fn initialised_mod(message: &str) -> Option<(&str, &str)> {
    let (id, version) = message.strip_suffix(INITIALISED)?.split_once(' ')?;
    (!id.is_empty() && !version.is_empty() && !version.contains(' ')).then_some((id, version))
}

/// How the line opens that a mod's log gets when the loader turns it off.
const OFF: &str = "off: ";

/// The line a mod's log gets when the loader turns it off for `reason`,
/// with the error `code`.
///
/// # Examples
///
/// ```
/// use enshwick_base::log_files::off;
///
/// assert_eq!(off("init returned 3", "E2003"), "off: init returned 3 (E2003)");
/// ```
#[must_use]
pub fn off(reason: &str, code: impl Display) -> String {
    format!("{OFF}{reason} ({code})")
}

/// The reason and code an [`off`] line gives; `None` for any other line.
///
/// # Examples
///
/// ```
/// use enshwick_base::log_files::off_reason;
///
/// assert_eq!(off_reason("off: init returned 3 (E2003)"), Some("init returned 3 (E2003)"));
/// assert_eq!(off_reason("you.hello 0.1.0 initialised"), None);
/// ```
#[must_use]
pub fn off_reason(message: &str) -> Option<&str> {
    message.strip_prefix(OFF)
}

/// How the loader's last line in a file that reached its size cap opens,
/// before the name of the file the run goes on in.
const ROLLED: &str = "this file reached its cap; the run goes on in ";

/// The loader's last line in a file that reached its size cap, naming the
/// file `next` the run goes on in.
///
/// # Examples
///
/// ```
/// use enshwick_base::log_files::rolled;
///
/// assert_eq!(rolled("b.log"), "this file reached its cap; the run goes on in b.log");
/// ```
#[must_use]
pub fn rolled(next: &str) -> String {
    format!("{ROLLED}{next}")
}

/// The file the last [`rolled`] line in `text` names; `None` when it holds
/// none.
///
/// # Examples
///
/// ```
/// use enshwick_base::log_files::{rolled, rolled_into};
///
/// let text = format!("00:00:00.000Z info  [enshwick] {}\n", rolled("b.log"));
/// assert_eq!(rolled_into(&text), Some("b.log"));
/// assert_eq!(rolled_into("no roll"), None);
/// ```
#[must_use]
pub fn rolled_into(text: &str) -> Option<&str> {
    let (_, next) = text.rsplit_once(ROLLED)?;
    Some(next.lines().next()?.trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_run_file_names_its_source_and_stamp() {
        let name = run_file_name("you.my-mod", "2026-11-03_10-00-00");
        assert_eq!(run_file(&name), Some(("you.my-mod", "2026-11-03_10-00-00")));
        assert_eq!(
            run_file("enshwick-crash_2026-11-03_10-00-00.log"),
            Some(("enshwick-crash", "2026-11-03_10-00-00"))
        );
        assert_eq!(run_file("start.log"), None);
        assert_eq!(run_file("enshwick_2026-11-03.log"), None);
        assert_eq!(run_file("_2026-11-03_10-00-00.log"), None);
        assert_eq!(run_file("enshwick_2026-11-03_10-00-00.txt"), None);
        assert_eq!(run_file("enshwick_2026x11-03_10-00-00.log"), None);
    }

    #[test]
    fn a_written_line_reads_back_as_its_time_level_source_and_message() {
        let mut text = String::from("before");
        let millis = (3 * 86_400 + 42 * 60 + 35) * 1000 + 449;
        let source = write_line(&mut text, millis, Level::ERROR, "you.pack", "off: no item");
        assert_eq!(&text[source], "you.pack");
        let written = text.strip_prefix("before").unwrap();
        assert_eq!(written, "00:42:35.449Z error [you.pack] off: no item\n");
        let line = Line::parse(written.trim_end()).unwrap();
        assert_eq!(
            line,
            Line {
                millis: (42 * 60 + 35) * 1000 + 449,
                level: Level::ERROR,
                source: "you.pack",
                message: "off: no item",
            }
        );
        assert_eq!(Line::parse("  at the second line"), None);
        assert_eq!(Line::parse("00:42:35.449Z loud  [you.pack] hi"), None);
        assert_eq!(Line::parse("99999999999:00:00.000Z info  [a] b"), None);
    }

    #[test]
    fn each_start_line_reads_back_as_what_it_was_written_with() {
        assert!(ready(2, 3).starts_with(READY));
        assert!(world_settings("D:/s/world0").starts_with(WORLD_SETTINGS));
        assert!(NO_WORLD_KEY.starts_with(WORLD_SETTINGS));
        assert_eq!(
            initialised_mod(&initialised("you.hello", "0.1.0")),
            Some(("you.hello", "0.1.0"))
        );
        assert_eq!(initialised_mod("everything initialised"), None);
        assert_eq!(initialised_mod(" 0.1.0 initialised"), None);
        assert_eq!(
            off_reason(&off("init returned 3", "E2003")),
            Some("init returned 3 (E2003)")
        );
        assert_eq!(off_reason("on: again"), None);
        let tail = format!("x\n00:00:00.000Z info  [enshwick] {}\r\n", rolled("b.log"));
        assert_eq!(rolled_into(&tail), Some("b.log"));
        assert_eq!(rolled_into("no roll here"), None);
    }
}
