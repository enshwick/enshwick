//! `logs/enshwick/` (`all.log`, `loader.log`, `mods/<id>.log`): one file for the loader and every mod.
//!
//! Callers only format a line and send it down a channel; a background thread
//! owns the file, so a hook never waits on the disk.

use std::fmt::Write as _;
use std::io::Write as _;
use std::path::Path;
use std::sync::OnceLock;
use std::sync::mpsc::{Sender, channel};
use std::time::{SystemTime, UNIX_EPOCH};

use arc_swap::ArcSwapOption;
use enshwick_loader_api::abi::Level;

use crate::settings::{LOADER_ID, Settings};

static LINES: OnceLock<Sender<(String, String)>> = OnceLock::new();
static LEVELS: ArcSwapOption<Settings> = ArcSwapOption::const_empty();

/// Keeps lines up to the levels in `settings`; before the first call every
/// source keeps `info`.
pub fn set_levels(settings: Settings) {
    LEVELS.store(Some(std::sync::Arc::new(settings)));
}

/// The most detailed level kept for lines from `source`.
#[must_use]
pub fn level(source: &str) -> Level {
    LEVELS
        .load()
        .as_ref()
        .map_or(Level::INFO, |s| s.level(source))
}

/// Whether a line at `level` from `source` would be kept, so a caller can
/// skip formatting one that would not.
#[must_use]
pub fn enabled(level: Level, source: &str) -> bool {
    level <= self::level(source)
}

/// Starts the writer in `dir`, truncating the previous session's files:
/// `all.log` gets every line, `loader.log` the loader's, `mods/<id>.log` each
/// mod's.
pub fn open(dir: &Path) {
    let mods = dir.join("mods");
    if std::fs::create_dir_all(&mods).is_err() {
        return;
    }
    let Ok(mut all) = std::fs::File::create(dir.join("all.log")) else {
        return;
    };
    let dir = dir.to_path_buf();
    let (send, receive) = channel::<(String, String)>();
    let started = std::thread::Builder::new()
        .name("enshwick-log".into())
        .spawn(move || {
            let mut files: std::collections::HashMap<String, Option<std::fs::File>> =
                std::collections::HashMap::new();
            for (source, line) in receive {
                let _all = all.write_all(line.as_bytes()).and_then(|()| all.flush());
                let file = files.entry(source).or_insert_with_key(|source| {
                    let path = if source == LOADER_ID {
                        dir.join("loader.log")
                    } else {
                        mods.join(format!("{source}.log"))
                    };
                    std::fs::File::create(path).ok()
                });
                // Flush per line: after a crash the last lines are the useful ones.
                if let Some(file) = file {
                    let _own = file.write_all(line.as_bytes()).and_then(|()| file.flush());
                }
            }
        });
    if started.is_ok() {
        let _first = LINES.set(send);
    }
}

/// Whether lines go to [`CAPTURED`] instead of the files.
static CAPTURING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// Lines kept in memory instead of written, for a dry run.
static CAPTURED: std::sync::Mutex<Option<Vec<(Level, String, String)>>> =
    std::sync::Mutex::new(None);

/// Keeps every line from now on in memory, for [`captured`].
pub fn capture() {
    if let Ok(mut lines) = CAPTURED.lock() {
        *lines = Some(Vec::new());
    }
    CAPTURING.store(true, std::sync::atomic::Ordering::Release);
}

/// The lines kept since [`capture`]: level, source and message.
#[must_use]
pub fn captured() -> Vec<(Level, String, String)> {
    CAPTURED
        .lock()
        .ok()
        .and_then(|lines| lines.clone())
        .unwrap_or_default()
}

/// Writes one line under `source`, the loader or a mod id.
pub fn write(level: Level, source: &str, message: &str) {
    if CAPTURING.load(std::sync::atomic::Ordering::Acquire) {
        if let Ok(mut lines) = CAPTURED.lock()
            && let Some(lines) = lines.as_mut()
        {
            lines.push((level, source.to_owned(), message.to_owned()));
        }
        return;
    }
    let Some(lines) = LINES.get() else {
        return;
    };
    if !enabled(level, source) {
        return;
    }
    let _sent = lines.send((
        source.to_owned(),
        format_line(now(), level, source, message),
    ));
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

fn format_line(millis: u64, level: Level, source: &str, message: &str) -> String {
    let secs = millis / 1000;
    let (h, m, s) = (secs / 3600 % 24, secs / 60 % 60, secs % 60);
    let level = level.name();
    let mut line = String::with_capacity(32 + source.len() + message.len());
    let _formatted = writeln!(
        line,
        "{h:02}:{m:02}:{s:02}.{:03}Z {level:<5} [{source}] {message}",
        millis % 1000
    );
    line
}

/// Logs a line from the loader itself; formats it only when its level is
/// kept.
#[macro_export]
macro_rules! log {
    ($level:ident, $($arg:tt)+) => {{
        let level = ::enshwick_loader_api::abi::Level::$level;
        if $crate::log::enabled(level, $crate::settings::LOADER_ID) {
            $crate::log::write(level, $crate::settings::LOADER_ID, &::std::format!($($arg)+));
        }
    }};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_below_its_source_s_level_is_not_formatted() {
        set_levels(Settings {
            log_level: Level::WARN,
            log_levels: [("chatty".to_owned(), Level::DEBUG)].into(),
            ..Settings::default()
        });
        assert!(enabled(Level::ERROR, LOADER_ID));
        assert!(!enabled(Level::INFO, LOADER_ID));
        assert!(enabled(Level::DEBUG, "chatty"));
        let formatted = core::cell::Cell::new(false);
        let costly = || {
            formatted.set(true);
            "costly"
        };
        crate::log!(DEBUG, "{}", costly());
        assert!(!formatted.get());
        crate::log!(ERROR, "{}", costly());
        assert!(formatted.get());
    }

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
