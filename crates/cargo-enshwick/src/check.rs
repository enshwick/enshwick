//! `cargo enshwick check`: checks a mod in any form with one command. A
//! data pack or Luau mod is read as the loader reads it and its scripts are
//! type-checked by luau-lsp; a Rust crate is linted by clippy for Windows,
//! which also runs the checks `declare_mod!` makes of its manifest.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context as _, bail};
use cargo_enshwick::luau_lsp;
use enshwick::packs::MANIFEST_FILE;

use crate::new::luau::DEFINITIONS_DIR;

/// Checks the mod in `dir`, with the vanilla names of the game in `game`
/// when given, writing each problem to `out`; a summary, or how many errors.
pub(crate) fn run(
    dir: &Path,
    game: Option<&Path>,
    offline: bool,
    out: &mut impl Write,
) -> anyhow::Result<String> {
    let pack = dir.join(MANIFEST_FILE).is_file();
    let crate_ = dir.join("Cargo.toml").is_file();
    if !pack && !crate_ {
        bail!(
            "{}: no {MANIFEST_FILE} or Cargo.toml; run this in a mod's folder or name it",
            dir.display()
        );
    }
    let mut errors = 0;
    let mut summary = Vec::new();
    if pack {
        let vanilla = game.map(crate::pack::vanilla).transpose()?;
        match crate::pack::check(dir, vanilla.as_ref()) {
            Ok((done, _, _)) => summary.push(done),
            Err(error) => {
                errors += 1;
                for line in format!("{error:#}").lines() {
                    writeln!(out, "{}", as_error(line))?;
                }
            }
        }
        let (found, done) = scripts(dir, offline)?;
        for finding in &found {
            writeln!(out, "{finding}")?;
        }
        errors += found.iter().filter(|f| f.error).count();
        summary.push(done);
    }
    if crate_ {
        if clippy(dir)? {
            summary.push("clippy found no errors".to_owned());
        } else {
            errors += 1;
        }
    }
    if errors > 0 {
        bail!(
            "{}: {errors} error{}",
            dir.display(),
            if errors == 1 { "" } else { "s" }
        );
    }
    Ok(summary.join("\n"))
}

/// A line of the pack reader's error as an error line: `error: ` after its
/// `<file>:<line>:<column>: ` when it starts with one, else in front.
fn as_error(line: &str) -> String {
    let mut parts = line.splitn(4, ':');
    if let (Some(file), Some(row), Some(column), Some(message)) =
        (parts.next(), parts.next(), parts.next(), parts.next())
        && row.parse::<u32>().is_ok()
        && column.parse::<u32>().is_ok()
        && let Some(message) = message.strip_prefix(' ')
    {
        return format!("{file}:{row}:{column}: error: {message}");
    }
    format!("error: {line}")
}

/// One problem luau-lsp found.
#[derive(Debug, PartialEq, Eq)]
struct Finding {
    file: String,
    line: u32,
    column: u32,
    error: bool,
    kind: String,
    message: String,
}

impl core::fmt::Display for Finding {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let Self {
            file,
            line,
            column,
            kind,
            message,
            ..
        } = self;
        if self.error {
            write!(f, "{file}:{line}:{column}: error: {message}")
        } else {
            write!(f, "{file}:{line}:{column}: warning: {message} [{kind}]")
        }
    }
}

/// Type-checks every script in `dir` with the definitions in its
/// `types/`: the problems, and a line saying what was checked.
fn scripts(dir: &Path, offline: bool) -> anyhow::Result<(Vec<Finding>, String)> {
    let files = luau_files(dir, Path::new(""))?;
    let (definitions, scripts): (Vec<String>, Vec<String>) = files
        .into_iter()
        .partition(|file| file.ends_with(".d.luau"));
    if scripts.is_empty() {
        return Ok((Vec::new(), "no Luau scripts to type-check".to_owned()));
    }
    let tool = match luau_lsp::find(offline) {
        Ok(tool) => tool,
        Err(why @ luau_lsp::Error::Offline(_)) => {
            return Ok((
                Vec::new(),
                format!("warning: scripts not type-checked: {why}"),
            ));
        }
        Err(why) => return Err(why.into()),
    };
    let prefix = format!("{DEFINITIONS_DIR}/");
    let definitions = definitions
        .iter()
        .filter(|file| file.starts_with(&prefix))
        .map(|file| {
            let name = file.trim_start_matches(&prefix).trim_end_matches(".d.luau");
            format!("--definitions:@{name}={file}")
        });
    let ran = Command::new(&tool)
        .current_dir(dir)
        .args(["analyze", "--formatter=gnu"])
        .args(definitions)
        .args(&scripts)
        .output()
        .with_context(|| format!("{} does not run", tool.display()))?;
    let text = String::from_utf8_lossy(&ran.stderr);
    let found = findings(&text);
    if !ran.status.success() && found.is_empty() {
        bail!("{} failed: {}", tool.display(), text.trim());
    }
    Ok((
        found,
        format!("type-checked {} Luau scripts", scripts.len()),
    ))
}

/// The `.luau` files under `dir`, below `at`, with `/` between folders,
/// sorted; hidden folders are left out.
fn luau_files(dir: &Path, at: &Path) -> anyhow::Result<Vec<String>> {
    let here = dir.join(at);
    let mut files = Vec::new();
    for entry in std::fs::read_dir(&here).with_context(|| here.display().to_string())? {
        let entry = entry.with_context(|| here.display().to_string())?;
        let name = entry.file_name();
        let path: PathBuf = at.join(&name);
        if entry.file_type().is_ok_and(|t| t.is_dir()) {
            if !name.to_string_lossy().starts_with('.') {
                files.extend(luau_files(dir, &path)?);
            }
        } else if name.to_string_lossy().ends_with(".luau") {
            files.push(path.to_string_lossy().replace('\\', "/"));
        }
    }
    files.sort();
    Ok(files)
}

/// The problems in luau-lsp's `--formatter=gnu` output, each a line
/// `<file>:<line>.<column>-<line>.<column>: <kind>: <message>` and the lines
/// that continue its message; its own log lines are left out.
fn findings(text: &str) -> Vec<Finding> {
    let mut found: Vec<Finding> = Vec::new();
    for line in text.lines() {
        if line.starts_with("[INFO]") || line.starts_with("[WARN]") {
            continue;
        }
        if let Some(finding) = finding(line) {
            found.push(finding);
        } else if let Some(last) = found.last_mut() {
            let more = line.trim();
            if !more.is_empty() {
                last.message.push(' ');
                last.message.push_str(more);
            }
        }
    }
    found
}

/// The problem a gnu-format line starts, if it starts one.
fn finding(line: &str) -> Option<Finding> {
    let (at, rest) = line.split_once(": ")?;
    let (file, range) = at.rsplit_once(':')?;
    let (line, column) = range.split('-').next()?.split_once('.')?;
    let (kind, message) = rest.split_once(": ")?;
    if kind.is_empty() || !kind.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return None;
    }
    Some(Finding {
        file: file.to_owned(),
        line: line.parse().ok()?,
        column: column.parse().ok()?,
        error: kind.ends_with("Error"),
        kind: kind.to_owned(),
        message: message.trim().to_owned(),
    })
}

/// Lints the crate in `dir` for Windows with clippy, its messages on the
/// terminal; whether it found no errors.
fn clippy(dir: &Path) -> anyhow::Result<bool> {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let windows = cfg!(windows);
    crate::install::preflight(
        windows,
        || {
            Command::new(&cargo)
                .args(["xwin", "--version"])
                .output()
                .is_ok_and(|out| out.status.success())
        },
        crate::install::windows_target_installed,
    )?;
    let xwin: &[&str] = if windows { &[] } else { &["xwin"] };
    let status = Command::new(&cargo)
        .args(xwin)
        .args([
            "clippy",
            "--all-targets",
            "--target",
            crate::install::WINDOWS_TARGET,
        ])
        .arg("--manifest-path")
        .arg(dir.join("Cargo.toml"))
        .status()
        .context("cargo clippy")?;
    Ok(status.success())
}

#[cfg(test)]
mod tests {
    use super::*;

    const OUTPUT: &str = "[INFO] Loading definitions file: @enshwick - types/enshwick.d.luau
bad.luau:4.1-4.0: SyntaxError: Expected identifier when parsing expression, got <eof>
bad.luau:1.13-1.25: TypeError: Expected this to be
\t'\"SettingsChanged\"'
but got
\t'\"PlayerJoind\"'
lib/count.luau:2.7-2.12: LocalUnused: Variable 'unused' is never used; prefix with '_' to silence
";

    #[test]
    fn luau_lsp_output_becomes_one_line_per_problem_with_its_severity() {
        let lines: Vec<String> = findings(OUTPUT).iter().map(ToString::to_string).collect();
        assert_eq!(
            lines,
            [
                "bad.luau:4:1: error: Expected identifier when parsing expression, got <eof>",
                "bad.luau:1:13: error: Expected this to be '\"SettingsChanged\"' but got '\"PlayerJoind\"'",
                "lib/count.luau:2:7: warning: Variable 'unused' is never used; prefix with '_' to silence [LocalUnused]",
            ]
        );
    }

    #[test]
    fn a_data_error_reads_as_an_error_at_its_place() {
        assert_eq!(
            as_error("items.toml:2:1: no item \"torch\""),
            "items.toml:2:1: error: no item \"torch\""
        );
        assert_eq!(
            as_error("pack.toml: version \"x\""),
            "error: pack.toml: version \"x\""
        );
    }

    #[test]
    fn a_bad_data_file_fails_the_check_at_its_line() {
        let dir = crate::pack::tests::pack(
            "check-bad",
            "[[item]]\nid = \"lantern\"\nlike = \"torch\"\n",
        );
        let mut out = Vec::new();
        let why = run(&dir, None, true, &mut out).unwrap_err().to_string();
        let out = String::from_utf8(out).unwrap();
        assert!(
            out.starts_with("items.toml:1:1: error: no item \"torch\""),
            "{out}"
        );
        assert!(why.ends_with(": 1 error"), "{why}");
        let _gone = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_folder_without_a_mod_is_refused() {
        let dir = std::env::temp_dir().join(format!("enshwick-check-none-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let why = run(&dir, None, true, &mut Vec::new())
            .unwrap_err()
            .to_string();
        assert!(why.contains("no pack.toml or Cargo.toml"), "{why}");
        let _gone = std::fs::remove_dir_all(&dir);
    }

    /// A new Luau mod in a scratch folder, with `main.luau` replaced by
    /// `main` when given.
    fn luau_mod(name: &str, main: Option<&str>) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("enshwick-check-{name}-{}", std::process::id()));
        let _old = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        crate::new::luau::create(&dir, "you.greeter", crate::Side::Server, None).unwrap();
        let root = dir.join("you.greeter");
        if let Some(main) = main {
            std::fs::write(root.join(enshwick::packs::SCRIPT_MAIN), main).unwrap();
        }
        root
    }

    /// Runs `check` on the new mod `dir`, then removes it: its result and
    /// what it wrote; `None` when luau-lsp cannot be had here, which CI
    /// never allows.
    fn checked(dir: &Path) -> Option<(anyhow::Result<String>, String)> {
        let found = luau_lsp::find(false);
        let mut out = Vec::new();
        let done = found.is_ok().then(|| run(dir, None, false, &mut out));
        let _gone = std::fs::remove_dir_all(dir.parent().unwrap());
        if let Err(why) = found {
            assert!(std::env::var_os("CI").is_none(), "{why}");
        }
        Some((done?, String::from_utf8(out).unwrap()))
    }

    #[test]
    fn a_new_luau_mod_checks_clean() {
        let dir = luau_mod("clean", None);
        let Some((done, out)) = checked(&dir) else {
            return;
        };
        let done = done.unwrap();
        assert!(
            done.contains("type-checked 1 Luau scripts"),
            "{done}\n{out}"
        );
        assert!(!out.contains("error"), "{out}");
    }

    #[test]
    fn a_misspelt_event_fails_naming_the_line_and_the_events() {
        let dir = luau_mod(
            "event",
            Some(
                "--!strict\nenshwick.on(\"PlayerJoind\", function(player)\n\tprint(player)\nend)\n",
            ),
        );
        let Some((done, out)) = checked(&dir) else {
            return;
        };
        assert!(done.is_err(), "{out}");
        assert!(out.contains("main.luau:2:13: error:"), "{out}");
        assert!(out.contains("\"PlayerJoined\""), "{out}");
    }

    #[test]
    fn a_syntax_error_fails_naming_the_line() {
        let dir = luau_mod("syntax", Some("--!strict\nlocal greeting =\n"));
        let Some((done, out)) = checked(&dir) else {
            return;
        };
        assert!(done.is_err(), "{out}");
        assert!(out.contains("main.luau:3:1: error:"), "{out}");
    }

    #[test]
    fn a_data_pack_needs_no_luau_lsp() {
        let dir = crate::pack::tests::pack(
            "check-data",
            "[[item]]\nid = \"lantern\"\nlike = \"6943053a-32c1-4718-ae22-e61427ddf54d\"\n",
        );
        let done = run(&dir, None, true, &mut Vec::new()).unwrap();
        assert!(done.contains("no Luau scripts"), "{done}");
        let _gone = std::fs::remove_dir_all(&dir);
    }
}
