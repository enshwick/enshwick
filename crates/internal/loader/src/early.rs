//! Lines from before the log opens, and panics: to [`FILE`] and the debugger.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// The file, in the log's folder beside the game or in the temporary folder
/// when the game's folder takes no files, that takes the lines; it keeps
/// earlier runs' lines, each dated, up to [`KEPT_BYTES`].
pub const FILE: &str = "start.log";
/// Past this size the file starts again empty.
const KEPT_BYTES: u64 = 64 << 10;
/// Panics the loader logs; later ones are counted only.
const LOGGED_PANICS: u32 = 64;
/// What a panic without a text message reads as.
const NO_MESSAGE: &str = "a panic without a message";

/// Writes `message` to the log when it is open, else to [`FILE`] and the
/// debugger's output.
pub fn note(message: &str) {
    if crate::log::is_open() {
        crate::log!(ERROR, "{message}");
    } else {
        write(message);
    }
}

/// Writes `message` to [`FILE`] and the debugger's output, past the log.
pub fn write(message: &str) {
    let line = line(now(), message);
    to_debugger(&line);
    let game_logs = std::env::current_exe().ok().and_then(|exe| {
        exe.parent()
            .map(|dir| dir.join(crate::log::LOGS).join(crate::settings::LOADER_ID))
    });
    let temp = std::env::temp_dir().join(crate::settings::LOADER_ID);
    let _kept = game_logs
        .into_iter()
        .chain([temp])
        .any(|dir| append(&dir, &line).is_ok());
}

/// `2026-10-29 14:03:09Z loader: message`, with a newline.
#[must_use]
pub fn line(unix_seconds: u64, message: &str) -> String {
    let (year, month, day) = civil(unix_seconds / 86_400);
    let time = crate::diag::clock(unix_seconds);
    format!("{year:04}-{month:02}-{day:02} {time} loader: {message}\n")
}

/// The proleptic Gregorian date `days` after 1970-01-01.
fn civil(days: u64) -> (u64, u64, u64) {
    let z = days + 719_468;
    let era = z / 146_097;
    let day_of_era = z % 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    let year = year_of_era + era * 400 + u64::from(month <= 2);
    (year, month, day)
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Appends `line` to [`FILE`] in `dir`, made if missing.
fn append(dir: &Path, line: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let path: PathBuf = dir.join(FILE);
    let full = std::fs::metadata(&path).is_ok_and(|m| m.len() > KEPT_BYTES);
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(!full)
        .write(true)
        .truncate(full)
        .open(path)?;
    file.write_all(line.as_bytes())
}

#[cfg(windows)]
fn to_debugger(line: &str) {
    let wide: Vec<u16> = line.encode_utf16().chain([0]).collect();
    // SAFETY: `wide` is a NUL-terminated UTF-16 string that outlives the call.
    unsafe { windows_sys::Win32::System::Diagnostics::Debug::OutputDebugStringW(wide.as_ptr()) };
}

#[cfg(not(windows))]
fn to_debugger(_line: &str) {}

/// Logs every panic of the loader's own code with its message, place and
/// thread, the first [`LOGGED_PANICS`] of them; the default hook would
/// write to a console the game does not have.
pub fn log_panics() {
    let seen = AtomicU32::new(0);
    std::panic::set_hook(Box::new(move |info| {
        let count = seen.fetch_add(1, Ordering::Relaxed);
        if count < LOGGED_PANICS {
            let message = info.payload_as_str().unwrap_or(NO_MESSAGE);
            let thread = std::thread::current();
            note(&panic_line(
                info.location().map(ToString::to_string).as_deref(),
                thread.name(),
                message,
            ));
        } else if count == LOGGED_PANICS {
            note("further panics are not logged");
        }
    }));
}

/// `panicked at src/ui.rs:4:9 on thread main: the message`, on one line.
#[must_use]
pub fn panic_line(at: Option<&str>, thread: Option<&str>, message: &str) -> String {
    let message = message
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("; ");
    let thread = thread.unwrap_or("without a name");
    match at {
        Some(at) => format!("panicked at {at} on thread {thread}: {message}"),
        None => format!("panicked on thread {thread}: {message}"),
    }
}

/// The first line of the message of the panic that left `payload`.
#[must_use]
pub fn panic_text(payload: &(dyn core::any::Any + Send)) -> &str {
    let text = payload
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
        .unwrap_or(NO_MESSAGE);
    text.lines().next().unwrap_or(NO_MESSAGE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_carries_the_date_and_time_in_utc() {
        assert_eq!(
            line(0, "the game's folder takes no files"),
            "1970-01-01 00:00:00Z loader: the game's folder takes no files\n"
        );
        assert_eq!(line(1_793_286_189, "x"), "2026-10-29 15:03:09Z loader: x\n");
        assert_eq!(line(951_782_400, "x"), "2000-02-29 00:00:00Z loader: x\n");
    }

    #[test]
    fn a_panic_line_names_place_thread_and_message_on_one_line() {
        assert_eq!(
            panic_line(
                Some("src/ui.rs:4:9"),
                Some("main"),
                "assertion failed\n  left: 1\n right: 2"
            ),
            "panicked at src/ui.rs:4:9 on thread main: assertion failed; left: 1; right: 2"
        );
        assert_eq!(
            panic_line(None, None, "boom"),
            "panicked on thread without a name: boom"
        );
    }

    #[test]
    fn a_panic_text_is_the_first_line_of_its_message() {
        let text: Box<dyn core::any::Any + Send> = Box::new(String::from("first\nsecond"));
        assert_eq!(panic_text(text.as_ref()), "first");
        let text: Box<dyn core::any::Any + Send> = Box::new("plain");
        assert_eq!(panic_text(text.as_ref()), "plain");
        let other: Box<dyn core::any::Any + Send> = Box::new(7_u32);
        assert_eq!(panic_text(other.as_ref()), NO_MESSAGE);
    }

    #[test]
    fn a_full_file_starts_again_and_a_short_one_keeps_its_lines() {
        let dir = std::env::temp_dir().join(format!("enshwick-early-{}", std::process::id()));
        append(&dir, "one\n").unwrap();
        append(&dir, "two\n").unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.join(FILE)).unwrap(),
            "one\ntwo\n"
        );
        std::fs::write(dir.join(FILE), vec![b'x'; 70_000]).unwrap();
        append(&dir, "three\n").unwrap();
        assert_eq!(std::fs::read_to_string(dir.join(FILE)).unwrap(), "three\n");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
