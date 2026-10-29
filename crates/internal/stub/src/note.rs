//! Why the stub could not start the loader: a line in [`FILE`] and the debugger.

use std::io::Write as _;
use std::path::Path;

use windows_sys::Win32::Foundation::SYSTEMTIME;
use windows_sys::Win32::System::Diagnostics::Debug::OutputDebugStringW;
use windows_sys::Win32::System::SystemInformation::GetSystemTime;

/// The folder, under the game's, of the loader's logs.
const LOGS: &str = r"logs\enshwick";
/// The folder, in the temporary folder, that takes the file when the game's
/// folder takes no files.
const TEMP_FOLDER: &str = "enshwick";
/// The file that takes the line; the loader writes its early lines there too.
const FILE: &str = "start.log";
/// Past this size the file starts again empty.
const KEPT_BYTES: u64 = 64 << 10;

/// Writes `message`, dated, wherever it can.
pub(crate) fn note(message: &str) {
    let mut now = SYSTEMTIME {
        wYear: 0,
        wMonth: 0,
        wDayOfWeek: 0,
        wDay: 0,
        wHour: 0,
        wMinute: 0,
        wSecond: 0,
        wMilliseconds: 0,
    };
    // SAFETY: `now` is a writable `SYSTEMTIME` on this stack.
    unsafe { GetSystemTime(&raw mut now) };
    let line = line(&now, message);
    let wide: Vec<u16> = line.encode_utf16().chain([0]).collect();
    // SAFETY: `wide` is a NUL-terminated UTF-16 string that outlives the call.
    unsafe { OutputDebugStringW(wide.as_ptr()) };
    let beside_game = crate::game_dir().map(|dir| dir.join(LOGS));
    let temp = std::env::temp_dir().join(TEMP_FOLDER);
    let _kept = beside_game
        .into_iter()
        .chain([temp])
        .any(|dir| append(&dir, &line).is_ok());
}

/// `2026-10-29 14:03:09Z stub: message`, with a newline.
fn line(at: &SYSTEMTIME, message: &str) -> String {
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}Z stub: {message}\n",
        at.wYear, at.wMonth, at.wDay, at.wHour, at.wMinute, at.wSecond
    )
}

/// Appends `line` to [`FILE`] in `dir`, made if missing.
fn append(dir: &Path, line: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(FILE);
    let full = std::fs::metadata(&path).is_ok_and(|m| m.len() > KEPT_BYTES);
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(!full)
        .write(true)
        .truncate(full)
        .open(path)?;
    file.write_all(line.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_carries_the_date_and_time_in_utc() {
        let at = SYSTEMTIME {
            wYear: 2026,
            wMonth: 10,
            wDayOfWeek: 4,
            wDay: 29,
            wHour: 14,
            wMinute: 3,
            wSecond: 9,
            wMilliseconds: 500,
        };
        assert_eq!(
            line(&at, "the loader is missing"),
            "2026-10-29 14:03:09Z stub: the loader is missing\n"
        );
    }

    #[test]
    fn a_full_file_starts_again_and_a_short_one_keeps_its_lines() {
        let dir = std::env::temp_dir().join(format!("enshwick-stub-note-{}", std::process::id()));
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
