//! `logs/enshwick/report.md`, the one file a player attaches to an issue.

use std::fmt::Write as _;
use std::path::Path;

/// The report's file name in `logs/enshwick/`.
#[cfg(windows)]
pub(crate) const FILE: &str = "report.md";
/// The latest warning and error lines the report keeps.
#[cfg(windows)]
pub(crate) const WARNINGS: usize = 200;
/// How long a report waits for the log to take the lines before it.
#[cfg(windows)]
const SETTLE: std::time::Duration = std::time::Duration::from_millis(500);

/// What the report says.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Report {
    /// When it was written, as the log writes times.
    pub(crate) at: String,
    /// The loader's version.
    pub(crate) version: String,
    /// The contract of the stub that started the loader.
    pub(crate) stub: u32,
    /// The system: Windows, or Wine with its version.
    pub(crate) system: String,
    /// The Wine DLL overrides, when set.
    pub(crate) overrides: Option<String>,
    /// Logical processors.
    pub(crate) cpus: usize,
    /// The status summary, once the loader serves the game.
    pub(crate) status: Option<String>,
    /// The files of `mods/` and `mods/chain/` with their sizes, `/`
    /// separated.
    pub(crate) files: Vec<(String, u64)>,
    /// The latest warning and error lines, oldest first.
    pub(crate) warnings: Vec<String>,
}

impl Report {
    /// The report as Markdown.
    pub(crate) fn render(&self) -> String {
        let mut out = String::with_capacity(4096);
        let _w = writeln!(out, "# Enshwick report\n");
        let _w = writeln!(
            out,
            "Written {}. It lists your mods and recent log lines; read it before you share it.\n",
            self.at
        );
        let _w = writeln!(out, "## Environment\n");
        let _w = writeln!(
            out,
            "- Enshwick {}, stub contract {}",
            self.version, self.stub
        );
        let _w = writeln!(out, "- {}, {} logical processors", self.system, self.cpus);
        if let Some(overrides) = &self.overrides {
            let _w = writeln!(out, "- WINEDLLOVERRIDES={overrides}");
        }
        let _w = writeln!(out, "\n## Status\n");
        match &self.status {
            Some(status) => fenced(&mut out, status),
            None => {
                let _w = writeln!(
                    out,
                    "The loader does not serve this game; see the warnings."
                );
            }
        }
        let _w = writeln!(out, "\n## Files\n");
        if self.files.is_empty() {
            let _w = writeln!(out, "No DLL in mods/.");
        }
        for (file, size) in &self.files {
            let _w = writeln!(out, "- `{file}`, {size} bytes");
        }
        let _w = writeln!(out, "\n## Warnings and errors\n");
        if self.warnings.is_empty() {
            let _w = writeln!(out, "None.");
        } else {
            fenced(&mut out, &self.warnings.join("\n"));
        }
        out
    }
}

/// `text` with the game's folder as `<game>`, the user's profile folder
/// `home` as `%USERPROFILE%`, and the user name of a Linux home folder, as
/// Wine shows it, as `<user>`; paths match in any ASCII case and with either
/// slash.
pub(crate) fn redact(text: &str, game_dir: &str, home: Option<&str>) -> String {
    let mut text = replace_path(text, game_dir, "<game>");
    if let Some(home) = home {
        text = replace_path(&text, home, "%USERPROFILE%");
    }
    linux_users(&text)
}

/// `text` with every occurrence of `path`, compared folded, as `with`.
fn replace_path(text: &str, path: &str, with: &str) -> String {
    let needle = fold_text(path.trim_end_matches(['/', '\\']));
    if needle.is_empty() {
        return text.to_owned();
    }
    let folded = fold_text(text);
    let mut out = String::with_capacity(text.len());
    let (mut at, mut i) = (0, 0);
    while i < folded.len() {
        match match_at(folded.as_bytes(), i, needle.as_bytes()) {
            Some(end) => {
                out.push_str(&text[at..i]);
                out.push_str(with);
                (at, i) = (end, end);
            }
            None => i += 1,
        }
    }
    out.push_str(&text[at..]);
    out
}

/// Where `needle` ends when it matches the folded `text` at `start`; each
/// separator of `needle` matches a run of them, as a path printed with
/// escaped backslashes has.
fn match_at(text: &[u8], start: usize, needle: &[u8]) -> Option<usize> {
    let mut at = start;
    for &want in needle {
        if want == b'\\' {
            let run = text[at..].iter().take_while(|&&b| b == b'\\').count();
            if run == 0 {
                return None;
            }
            at += run;
        } else if text.get(at) == Some(&want) {
            at += 1;
        } else {
            return None;
        }
    }
    Some(at)
}

/// `text` folded for comparing paths: ASCII lower case and `/` as `\`, its
/// other characters kept, so its byte offsets are `text`'s.
fn fold_text(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '/' => '\\',
            c => c.to_ascii_lowercase(),
        })
        .collect()
}

/// `text` with the name after each `home` folder of a Linux path, as
/// `/home/ann` or Wine's `Z:\home\ann`, as `<user>`.
fn linux_users(text: &str) -> String {
    let folded = fold_text(text);
    let bytes = folded.as_bytes();
    let mut out = String::with_capacity(text.len());
    let (mut at, mut i) = (0, 0);
    while i < bytes.len() {
        let Some(name) = match_at(bytes, i, b"\\home\\") else {
            i += 1;
            continue;
        };
        let end = folded[name..]
            .find(|c: char| c == '\\' || c.is_whitespace() || c == '`' || c == '"')
            .map_or(text.len(), |len| name + len);
        out.push_str(&text[at..name]);
        out.push_str("<user>");
        (at, i) = (end, end);
    }
    out.push_str(&text[at..]);
    out
}

/// `text` as a fenced block that no line of it can close.
fn fenced(out: &mut String, text: &str) {
    let longest = text
        .lines()
        .map(|line| line.chars().take_while(|&c| c == '`').count())
        .max()
        .unwrap_or(0);
    let fence = "`".repeat(longest.max(2) + 1);
    let _w = writeln!(out, "{fence}text\n{}\n{fence}", text.trim_end());
}

/// Writes the report for the game in `game_dir`, started by a stub of
/// contract `stub`, with `status` once the loader serves the game. It first
/// waits a moment for the log to take the lines before it. Not on a game
/// thread.
#[cfg(windows)]
pub(crate) fn write(game_dir: &Path, stub: u32, status: Option<String>) {
    let _settled = crate::log::settle(SETTLE);
    let mods_dir = game_dir.join("mods");
    let mut files = files_in(&mods_dir, "mods/");
    files.extend(files_in(&mods_dir.join("chain"), "mods/chain/"));
    files.extend(
        crate::others::beside_game(game_dir)
            .into_iter()
            .filter_map(|name| {
                let size = std::fs::metadata(game_dir.join(&name)).ok()?.len();
                Some((name, size))
            }),
    );
    let report = Report {
        at: crate::diag::clock(crate::diag::now()),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        stub,
        system: system(),
        overrides: std::env::var("WINEDLLOVERRIDES").ok(),
        cpus: std::thread::available_parallelism().map_or(0, std::num::NonZero::get),
        status,
        files,
        warnings: crate::log::recent(None, enshwick::abi::Level::WARN, WARNINGS),
    };
    let home = std::env::var("USERPROFILE").ok();
    let text = redact(
        &report.render(),
        &game_dir.to_string_lossy(),
        home.as_deref(),
    );
    let dir = game_dir.join("logs").join(crate::settings::LOADER_ID);
    let fresh = dir.join(format!("{FILE}.tmp"));
    let written =
        std::fs::write(&fresh, text).and_then(|()| std::fs::rename(&fresh, dir.join(FILE)));
    if let Err(e) = written {
        crate::log!(WARN, "{FILE} not written: {e}");
    }
}

/// Windows, or Wine with its version when the process runs under Wine.
#[cfg(windows)]
fn system() -> String {
    use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
    type WineVersion = unsafe extern "C" fn() -> *const core::ffi::c_char;
    // SAFETY: a NUL-terminated name; ntdll is loaded in every process.
    let module = unsafe { GetModuleHandleW(windows_sys::core::w!("ntdll.dll")) };
    if module.is_null() {
        return "Windows".to_owned();
    }
    // SAFETY: `module` is loaded and the name is NUL-terminated.
    let Some(found) = (unsafe { GetProcAddress(module, c"wine_get_version".as_ptr().cast()) })
    else {
        return "Windows".to_owned();
    };
    // SAFETY: Wine's ntdll (dlls/ntdll) defines it as
    // `const char * CDECL wine_get_version(void)`; CDECL is the C calling
    // convention on x64, which `WineVersion` names.
    let version =
        unsafe { core::mem::transmute::<unsafe extern "system" fn() -> isize, WineVersion>(found) };
    // SAFETY: the function takes no arguments, touches no thread state and
    // only returns the address of a static string, on any thread.
    let text = unsafe { version() };
    if text.is_null() {
        return "Wine".to_owned();
    }
    // SAFETY: a static NUL-terminated string from Wine, valid for the process.
    let text = unsafe { core::ffi::CStr::from_ptr(text) };
    format!("Wine {}", text.to_string_lossy())
}

/// The DLLs directly in `dir`, by name with their sizes, prefixed with
/// `prefix`, sorted.
fn files_in(dir: &Path, prefix: &str) -> Vec<(String, u64)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<(String, u64)> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let meta = entry.metadata().ok()?;
            let name = entry.file_name().to_string_lossy().into_owned();
            let dll = Path::new(&name)
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("dll"));
            (meta.is_file() && dll).then(|| (format!("{prefix}{name}"), meta.len()))
        })
        .collect();
    files.sort();
    files
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report() -> Report {
        Report {
            at: "12:30:00Z".into(),
            version: "1.0.0-alpha.2".into(),
            stub: 2,
            system: "Wine 9.0 (Proton)".into(),
            overrides: Some("powrprof=n,b".into()),
            cpus: 8,
            status: Some("mods: 1 of 1 running".into()),
            files: vec![("mods/lanterns.dll".into(), 4096)],
            warnings: vec![
                r"12:00:00.000Z WARN  [enshwick] C:\Users\ann\game\mods/x.dll could not be loaded"
                    .into(),
            ],
        }
    }

    #[test]
    fn the_report_holds_environment_status_files_and_warnings() {
        let text = report().render();
        assert!(text.starts_with("# Enshwick report\n"));
        assert!(text.contains("- Enshwick 1.0.0-alpha.2, stub contract 2\n"));
        assert!(text.contains("- Wine 9.0 (Proton), 8 logical processors\n"));
        assert!(text.contains("- WINEDLLOVERRIDES=powrprof=n,b\n"));
        assert!(text.contains("```text\nmods: 1 of 1 running\n```"));
        assert!(text.contains("- `mods/lanterns.dll`, 4096 bytes\n"));
        assert!(text.contains("could not be loaded\n```"));
    }

    #[test]
    fn paths_show_the_game_folder_and_the_profile_without_the_user_name() {
        let text = "D:\\Games\\Enshrouded\\mods/x.dll; d:/games/enshrouded/logs; \
                    c:\\users\\ann\\AppData; C:/Users/Ann/x";
        let shown = redact(text, "D:\\Games\\Enshrouded", Some("C:\\Users\\ann"));
        assert_eq!(
            shown,
            "<game>\\mods/x.dll; <game>/logs; %USERPROFILE%\\AppData; %USERPROFILE%/x"
        );
    }

    #[test]
    fn a_linux_home_under_wine_shows_without_the_user_name() {
        let text = "Z:\\home\\ann\\.local\\share\\Steam and /home/ann/x and `/home/bob`";
        assert_eq!(
            redact(text, "C:\\nowhere", None),
            "Z:\\home\\<user>\\.local\\share\\Steam and /home/<user>/x and `/home/<user>`"
        );
    }

    #[test]
    fn a_path_printed_with_escaped_backslashes_shows_without_the_user_name() {
        assert_eq!(
            redact(
                r"C:\\Users\\ann\\AppData\\x",
                "D:\\Games",
                Some("C:\\Users\\ann")
            ),
            r"%USERPROFILE%\\AppData\\x"
        );
        assert_eq!(
            redact(r#""Z:\\home\\ann\\.steam\\x""#, "D:\\Games", None),
            r#""Z:\\home\\<user>\\.steam\\x""#
        );
    }

    #[test]
    fn the_files_are_the_dlls_of_a_folder_by_name() {
        let dir = std::env::temp_dir().join(format!("enshwick-report-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("sub.dll")).unwrap();
        for (file, size) in [("b.DLL", 3), ("a.dll", 1), ("notes.txt", 2)] {
            std::fs::write(dir.join(file), vec![0u8; size]).unwrap();
        }
        let files = files_in(&dir, "mods/");
        let _removed = std::fs::remove_dir_all(&dir);
        assert_eq!(
            files,
            [("mods/a.dll".to_owned(), 1), ("mods/b.DLL".to_owned(), 3)]
        );
        assert!(files_in(&dir.join("gone"), "mods/").is_empty());
    }

    #[test]
    fn a_game_the_loader_does_not_serve_still_gets_a_report() {
        let text = Report {
            status: None,
            files: Vec::new(),
            warnings: Vec::new(),
            ..report()
        }
        .render();
        assert!(text.contains("The loader does not serve this game"));
        assert!(text.contains("No DLL in mods/."));
        assert!(text.contains("## Warnings and errors\n\nNone."));
    }

    #[test]
    fn a_log_line_with_backticks_cannot_close_its_block() {
        let mut out = String::new();
        fenced(&mut out, "a line\n```` b");
        assert!(out.starts_with("`````text\n"), "{out}");
    }
}
