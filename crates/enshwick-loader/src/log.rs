//! `mods/enshwick.log`: one file for the loader and every mod.
//!
//! Callers only format a line and send it down a channel; a background thread
//! owns the file, so a hook never waits on the disk.

use std::fmt::Write as _;
use std::io::Write as _;
use std::path::Path;
use std::sync::OnceLock;
use std::sync::mpsc::{Sender, channel};
use std::time::{SystemTime, UNIX_EPOCH};

use enshwick_api::abi::Level;

static LINES: OnceLock<Sender<String>> = OnceLock::new();

/// Starts the writer on `path`, truncating the previous session's log.
pub fn open(path: &Path) {
    let Ok(mut file) = std::fs::File::create(path) else {
        return;
    };
    let (send, receive) = channel::<String>();
    let started = std::thread::Builder::new()
        .name("enshwick-log".into())
        .spawn(move || {
            for line in receive {
                if file.write_all(line.as_bytes()).is_err() {
                    break;
                }
                // Flush per line: after a crash the last lines are the useful ones.
                let _flushed = file.flush();
            }
        });
    if started.is_ok() {
        let _first = LINES.set(send);
    }
}

/// Writes one line under `source`, the loader or a mod id.
pub fn write(level: Level, source: &str, message: &str) {
    let Some(lines) = LINES.get() else {
        return;
    };
    let _sent = lines.send(format_line(now(), level, source, message));
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

fn format_line(millis: u64, level: Level, source: &str, message: &str) -> String {
    let secs = millis / 1000;
    let (h, m, s) = (secs / 3600 % 24, secs / 60 % 60, secs % 60);
    let level = match level {
        Level::ERROR => "error",
        Level::WARN => "warn",
        Level::INFO => "info",
        _ => "debug",
    };
    let mut line = String::with_capacity(32 + source.len() + message.len());
    let _formatted = writeln!(
        line,
        "{h:02}:{m:02}:{s:02}.{:03}Z {level:<5} [{source}] {message}",
        millis % 1000
    );
    line
}

/// Logs a line from the loader itself.
#[macro_export]
macro_rules! log {
    ($level:ident, $($arg:tt)+) => {
        $crate::log::write(::enshwick_api::abi::Level::$level, "enshwick", &::std::format!($($arg)+))
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_names_its_time_level_and_source() {
        let line = format_line(
            3_723_004,
            Level::WARN,
            "flame-altar-zones",
            "merged 3 altars",
        );
        assert_eq!(
            line,
            "01:02:03.004Z warn  [flame-altar-zones] merged 3 altars\n"
        );
    }
}
