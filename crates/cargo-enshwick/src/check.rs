//! `cargo enshwick check`: checks a mod in any form with one command. A
//! data pack or Luau mod is read as the loader reads it and its scripts are
//! type-checked by luau-lsp; a Rust crate is linted by clippy for Windows,
//! which also runs the checks `declare_mod!` makes of its manifest.

use std::io::Write;
use std::path::Path;
use std::process::Command;

use anyhow::{Context as _, bail};
use cargo_enshwick::luau_lsp;
use enshwick_core::packs::MANIFEST_FILE;

use crate::new::luau::{DEFINITION_ALIASES, DEFINITIONS_DIR, PLATFORM};

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
                for line in format!("{error:#}").lines() {
                    errors += 1;
                    writeln!(out, "{}", as_error(line))?;
                }
            }
        }
        let (found, done) = scripts(dir, offline, out)?;
        errors += found;
        summary.push(done);
    }
    if crate_ {
        if clippy(dir)? {
            summary.push("clippy found no errors".to_owned());
        } else {
            errors += 1;
            writeln!(out, "error: clippy found errors")?;
        }
    }
    if errors > 0 {
        // What did pass, and what went unchecked, still matters to the author.
        for line in &summary {
            writeln!(out, "{line}")?;
        }
        let name = dir
            .canonicalize()
            .ok()
            .and_then(|dir| dir.file_name().map(|n| n.to_string_lossy().into_owned()))
            .unwrap_or_else(|| dir.display().to_string());
        bail!(
            "{name}: {errors} error{}",
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

/// Type-checks the scripts the loader runs from `dir` with the definitions
/// in its `types/`, as the editor settings of a new mod do, writing each
/// problem to `out`: how many are errors, and a line saying what was
/// checked.
fn scripts(dir: &Path, offline: bool, out: &mut impl Write) -> anyhow::Result<(usize, String)> {
    let scripts: Vec<String> = enshwick_core::packs::script_files(dir)?
        .iter()
        .map(|path| relative(dir, path))
        .collect();
    if scripts.is_empty() {
        return Ok((0, "no Luau scripts to type-check".to_owned()));
    }
    let tool = match luau_lsp::find(offline) {
        Ok(tool) => tool,
        Err(why @ luau_lsp::Error::Offline(_)) => {
            return Ok((0, format!("warning: scripts not type-checked: {why}")));
        }
        Err(why) => {
            writeln!(out, "error: scripts not type-checked: {why}")?;
            return Ok((1, "scripts not type-checked".to_owned()));
        }
    };
    let ran = Command::new(&tool)
        .current_dir(dir)
        .args(analyze_args(&definitions(dir)?, &scripts))
        .output()
        .with_context(|| format!("{} does not run", tool.display()))?;
    let text = String::from_utf8_lossy(&ran.stderr);
    let (found, failed) = judged(ran.status.success(), &text);
    for finding in &found {
        writeln!(out, "{finding}")?;
    }
    if let Some(failed) = &failed {
        writeln!(out, "error: {} {failed}", tool.display())?;
    }
    Ok((
        found.iter().filter(|f| f.error).count() + usize::from(failed.is_some()),
        format!(
            "type-checked {} Luau script{}",
            scripts.len(),
            if scripts.len() == 1 { "" } else { "s" }
        ),
    ))
}

/// `path` below `dir`, with `/` between folders.
fn relative(dir: &Path, path: &Path) -> String {
    path.strip_prefix(dir)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// The definitions files in `dir`'s `types/`, sorted.
fn definitions(dir: &Path) -> anyhow::Result<Vec<String>> {
    let types = dir.join(DEFINITIONS_DIR);
    if !types.is_dir() {
        return Ok(Vec::new());
    }
    let mut files = Vec::new();
    for entry in std::fs::read_dir(&types).with_context(|| types.display().to_string())? {
        let name = entry
            .with_context(|| types.display().to_string())?
            .file_name()
            .to_string_lossy()
            .into_owned();
        if name.ends_with(enshwick_core::packs::DEFINITIONS_SUFFIX) {
            files.push(format!("{DEFINITIONS_DIR}/{name}"));
        }
    }
    files.sort();
    Ok(files)
}

/// The arguments of `luau-lsp analyze` over `scripts` with `definitions`,
/// each under the alias the editor settings of a new mod give it.
fn analyze_args(definitions: &[String], scripts: &[String]) -> Vec<String> {
    let mut args = vec![
        "analyze".to_owned(),
        "--formatter=gnu".to_owned(),
        format!("--platform={PLATFORM}"),
    ];
    for file in definitions {
        let name = file.rsplit('/').next().unwrap_or(file);
        let alias = DEFINITION_ALIASES
            .iter()
            .find(|(of, _)| *of == name)
            .map_or_else(
                || name.trim_end_matches(enshwick_core::packs::DEFINITIONS_SUFFIX),
                |(_, alias)| alias,
            );
        args.push(format!("--definitions:@{alias}={file}"));
    }
    args.extend(scripts.iter().cloned());
    args
}

/// The problems of a luau-lsp run that wrote `text`, and why the run
/// counts as failed beyond them: luau-lsp also fails on lints a `.luaurc`
/// makes errors and on files it cannot open.
fn judged(success: bool, text: &str) -> (Vec<Finding>, Option<String>) {
    let found = findings(text);
    let failed = (!success && !found.iter().any(|f| f.error)).then(|| {
        if found.is_empty() {
            format!("failed: {}", text.trim())
        } else {
            "failed: it counts a problem above as an error".to_owned()
        }
    });
    (found, failed)
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
        || crate::install::windows_target_installed(dir),
    )?;
    let xwin: &[&str] = if windows { &[] } else { &["xwin"] };
    // The mod's own .cargo/config.toml applies only from its folder.
    let status = Command::new(&cargo)
        .current_dir(dir)
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
    use std::path::PathBuf;

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
    fn a_misspelt_field_of_a_reward_is_named_with_the_one_it_is_near() {
        let dir = crate::pack::tests::pack("check-field", "");
        std::fs::write(
            dir.join("rewards.toml"),
            "[[reward]]\nid = \"torch\"\non = { join = true }\ngive = [{ item = \"Tool_Torch99\", cout = 1 }]\n",
        )
        .unwrap();
        let mut out = Vec::new();
        assert!(run(&dir, None, true, &mut out).is_err());
        let out = String::from_utf8(out).unwrap();
        assert!(
            out.starts_with(
                "rewards.toml:4:8: error: unknown field `cout`, expected `item` or `count`; near: count"
            ),
            "{out}"
        );
        let _gone = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_folder_without_a_mod_is_refused() {
        let dir = crate::scratch("none");
        let why = run(&dir, None, true, &mut Vec::new())
            .unwrap_err()
            .to_string();
        assert!(why.contains("no pack.toml or Cargo.toml"), "{why}");
        let _gone = std::fs::remove_dir_all(&dir);
    }

    /// A new Luau mod in a scratch folder, with `main.luau` replaced by
    /// `main` when given.
    fn luau_mod(name: &str, main: Option<&str>) -> PathBuf {
        let dir = crate::scratch(name);
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
        assert!(done.contains("type-checked 1 Luau script"), "{done}\n{out}");
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
    fn a_log_line_after_a_problem_stays_out_of_its_message() {
        let found =
            findings("main.luau:1.1-1.2: TypeError: wrong\n[INFO] done\n[WARN] slow\n  more\n");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].message, "wrong more");
    }

    #[test]
    fn a_line_whose_kind_is_not_one_word_starts_no_problem() {
        assert_eq!(finding("main.luau:1.1-1.2: not a kind: wrong"), None);
        assert_eq!(finding("main.luau:1.1-1.2: : wrong"), None);
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

    #[test]
    fn a_failing_run_fails_the_check_even_when_it_names_only_lints() {
        let lint = "main.luau:2.7-2.12: LocalUnused: Variable 'x' is never used\n";
        let (found, failed) = judged(false, lint);
        assert_eq!(found.len(), 1);
        assert!(!found[0].error);
        assert_eq!(
            failed.as_deref(),
            Some("failed: it counts a problem above as an error")
        );
        assert_eq!(judged(true, lint).1, None);
        assert_eq!(
            judged(false, "Error opening main.luau\n").1.as_deref(),
            Some("failed: Error opening main.luau")
        );
        let error = "main.luau:1.1-1.2: TypeError: wrong\n";
        assert_eq!(judged(false, error).1, None, "the error already counts");
    }

    #[test]
    fn check_type_checks_as_the_editor_settings_of_a_new_mod_do() {
        let files =
            crate::new::luau::files("you.greeter", crate::Side::Server, Some(String::new()));
        let settings = &files
            .iter()
            .find(|(path, _)| path.ends_with("settings.json"))
            .unwrap()
            .1;
        let settings: serde_json::Value = serde_json::from_str(settings).unwrap();
        let aliases = settings["luau-lsp.types.definitionFiles"]
            .as_object()
            .unwrap();
        let definitions: Vec<String> = aliases
            .values()
            .map(|path| path.as_str().unwrap().to_owned())
            .collect();
        let args = analyze_args(&definitions, &["main.luau".to_owned()]);
        assert!(
            args.contains(&format!(
                "--platform={}",
                settings["luau-lsp.platform.type"].as_str().unwrap()
            )),
            "{args:?}"
        );
        for (alias, path) in aliases {
            let arg = format!("--definitions:{alias}={}", path.as_str().unwrap());
            assert!(args.contains(&arg), "{arg} in {args:?}");
        }
    }

    #[test]
    fn only_the_scripts_the_loader_runs_are_type_checked() {
        // Without main.luau the loader runs no script of the pack.
        let dir = crate::pack::tests::pack(
            "check-no-main",
            "[[item]]\nid = \"lantern\"\nlike = \"6943053a-32c1-4718-ae22-e61427ddf54d\"\n",
        );
        std::fs::create_dir_all(dir.join("lib")).unwrap();
        std::fs::write(dir.join("lib/count.luau"), "local x =\n").unwrap();
        let done = run(&dir, None, false, &mut Vec::new()).unwrap();
        assert!(done.contains("no Luau scripts"), "{done}");
        let _gone = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_failed_check_still_says_what_it_checked() {
        let dir = crate::pack::tests::pack(
            "check-said",
            "[[item]]\nid = \"lantern\"\nlike = \"torch\"\n",
        );
        let mut out = Vec::new();
        assert!(run(&dir, None, true, &mut out).is_err());
        let out = String::from_utf8(out).unwrap();
        assert!(out.ends_with("no Luau scripts to type-check\n"), "{out}");
        let _gone = std::fs::remove_dir_all(&dir);
    }
}
