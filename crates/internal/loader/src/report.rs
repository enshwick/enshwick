//! `logs/enshwick/report.md`: what a player attaches to an issue, in one
//! file. It holds the loader and game versions, the system it runs on, the
//! status summary with the last session's crash, the files of `mods/`, and
//! the latest warnings and errors. The loader writes it once its log opens, again when it
//! cannot serve the game, and with every status summary after.

use std::fmt::Write as _;
use std::path::Path;

/// The report's file name in `logs/enshwick/`.
pub(crate) const FILE: &str = "report.md";
/// The latest warning and error lines the report keeps.
pub(crate) const WARNINGS: usize = 200;

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
    /// The report as Markdown, with `home`, the user's profile folder, as
    /// `%USERPROFILE%` wherever it shows.
    pub(crate) fn render(&self, home: Option<&str>) -> String {
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
        match home.filter(|home| !home.is_empty()) {
            Some(home) => out.replace(home, "%USERPROFILE%"),
            None => out,
        }
    }
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

/// What the report says of the process, known from the loader's start.
#[cfg(windows)]
#[derive(Debug)]
struct Facts {
    game_dir: std::path::PathBuf,
    stub: u32,
}

#[cfg(windows)]
static FACTS: std::sync::OnceLock<Facts> = std::sync::OnceLock::new();

/// Keeps what the report says of the process; the first call wins.
#[cfg(windows)]
pub(crate) fn begin(game_dir: &Path, stub: u32) {
    let _first = FACTS.set(Facts {
        game_dir: game_dir.to_path_buf(),
        stub,
    });
}

/// Writes the report, with `status` once the loader serves the game.
#[cfg(windows)]
pub(crate) fn write(status: Option<String>) {
    let Some(facts) = FACTS.get() else {
        return;
    };
    let mods_dir = facts.game_dir.join("mods");
    let mut files = files_in(&mods_dir, "mods/");
    files.extend(files_in(&mods_dir.join("chain"), "mods/chain/"));
    let report = Report {
        at: crate::diag::clock(crate::diag::now()),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        stub: facts.stub,
        system: system(),
        overrides: std::env::var("WINEDLLOVERRIDES").ok(),
        cpus: std::thread::available_parallelism().map_or(0, std::num::NonZero::get),
        status,
        files,
        warnings: crate::log::recent(None, enshwick::abi::Level::WARN, WARNINGS),
    };
    let home = std::env::var("USERPROFILE").ok();
    let path = facts
        .game_dir
        .join("logs")
        .join(crate::settings::LOADER_ID)
        .join(FILE);
    if let Err(e) = std::fs::write(&path, report.render(home.as_deref())) {
        crate::log!(WARN, "{FILE} not written: {e}");
    }
}

/// Windows, or Wine with its version when the process runs under Wine.
#[cfg(windows)]
fn system() -> String {
    use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
    type WineVersion = unsafe extern "C" fn() -> *const core::ffi::c_char;
    let ntdll: Vec<u16> = "ntdll.dll".encode_utf16().chain([0]).collect();
    // SAFETY: a NUL-terminated name; ntdll is loaded in every process.
    let module = unsafe { GetModuleHandleW(ntdll.as_ptr()) };
    if module.is_null() {
        return "Windows".to_owned();
    }
    // SAFETY: `module` is loaded and the name is NUL-terminated.
    let Some(found) = (unsafe { GetProcAddress(module, c"wine_get_version".as_ptr().cast()) })
    else {
        return "Windows".to_owned();
    };
    // SAFETY: Wine's ntdll exports `wine_get_version` as a C function that
    // takes nothing and returns a static NUL-terminated string.
    let version: WineVersion = unsafe { core::mem::transmute(found) };
    // SAFETY: as above, called on any thread.
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
pub(crate) fn files_in(dir: &Path, prefix: &str) -> Vec<(String, u64)> {
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
        let text = report().render(None);
        assert!(text.starts_with("# Enshwick report\n"));
        assert!(text.contains("- Enshwick 1.0.0-alpha.2, stub contract 2\n"));
        assert!(text.contains("- Wine 9.0 (Proton), 8 logical processors\n"));
        assert!(text.contains("- WINEDLLOVERRIDES=powrprof=n,b\n"));
        assert!(text.contains("```text\nmods: 1 of 1 running\n```"));
        assert!(text.contains("- `mods/lanterns.dll`, 4096 bytes\n"));
        assert!(text.contains("could not be loaded\n```"));
    }

    #[test]
    fn the_report_names_the_profile_folder_without_the_user_name() {
        let text = report().render(Some(r"C:\Users\ann"));
        assert!(!text.contains("ann"), "{text}");
        assert!(text.contains(r"%USERPROFILE%\game\mods/x.dll"));
    }

    #[test]
    fn a_game_the_loader_does_not_serve_still_gets_a_report() {
        let text = Report {
            status: None,
            files: Vec::new(),
            warnings: Vec::new(),
            ..report()
        }
        .render(None);
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
