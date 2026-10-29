//! Why the stub could not start the loader, for the player to find: one line
//! in `logs\enshwick\start.log` beside the game, or in `enshwick\start.log`
//! in the temporary folder when the game's folder takes no files, and in the
//! debugger's output. The loader writes its own early lines to the same
//! file.

use std::io::Write as _;
use std::path::Path;

use windows_sys::Win32::Foundation::SYSTEMTIME;
use windows_sys::Win32::System::Diagnostics::Debug::OutputDebugStringW;
use windows_sys::Win32::System::SystemInformation::GetSystemTime;

/// The folder, under the game's, of the loader's logs.
const LOGS: &str = r"logs\enshwick";
/// The file that takes the line.
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
    let line = format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}Z stub: {message}\n",
        now.wYear, now.wMonth, now.wDay, now.wHour, now.wMinute, now.wSecond
    );
    let wide: Vec<u16> = line.encode_utf16().chain([0]).collect();
    // SAFETY: `wide` is a NUL-terminated UTF-16 string that outlives the call.
    unsafe { OutputDebugStringW(wide.as_ptr()) };
    let beside_game = crate::game_dir().map(|dir| dir.join(LOGS));
    let temp = std::env::temp_dir().join("enshwick");
    let _kept = beside_game
        .into_iter()
        .chain([temp])
        .any(|dir| append(&dir, &line).is_ok());
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
