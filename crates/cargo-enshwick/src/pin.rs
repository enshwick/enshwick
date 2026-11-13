//! `cargo enshwick pin`: ports a mod's own build rows to a new game build.
//!
//! Given the executable the current rows were made for and a new one, it
//! re-finds every pinned symbol of each `[[package.metadata.enshwick.builds]]`
//! row in the new executable, checks the constants against the code that
//! uses them and writes the new rows into each `Cargo.toml`, with a report.
//! It never guesses: whatever it cannot place for sure is left out, with a
//! note, and listed for a human. The porting itself is shared with the
//! maintainers' `cargo xtask compat pin`, which ports the loader's entries.

pub mod anchor;
pub mod exe;
pub mod patterns;
pub mod port;
pub mod report;
pub mod row;
pub mod rows;
pub mod signature;
pub mod staged;

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, bail};
use enshwick_gamedata::reflect::Fingerprint;

use self::exe::Exe;
use self::rows::{Row, Side, guid_hex};
use self::staged::Staged;

/// What `cargo enshwick pin` is given.
#[derive(Debug, clap::Args)]
pub struct Args {
    /// The executable the current rows were made for.
    #[arg(long)]
    pub old: PathBuf,
    /// The new executable.
    #[arg(long)]
    pub new: PathBuf,
    /// The mod or workspace whose rows to port: every `Cargo.toml` below it.
    #[arg(long, default_value = ".")]
    pub dir: PathBuf,
    /// Inserts each new row after its old one in its `Cargo.toml`.
    #[arg(long)]
    pub write: bool,
    /// Writes the report here instead of to standard output.
    #[arg(long)]
    pub report: Option<PathBuf>,
}

/// The exit code when the run worked but some entries need a human.
pub const NEEDS_HUMAN: u8 = 2;

/// Reads the file at `path`, naming it in the error.
///
/// # Errors
///
/// When the file does not read.
pub fn read(path: &Path) -> anyhow::Result<Vec<u8>> {
    fs::read(path).with_context(|| path.display().to_string())
}

/// Ports the rows of every `Cargo.toml` below `args.dir`; `Ok(true)` when
/// nothing needs a human.
///
/// # Errors
///
/// When a file does not read or parse, the executables carry no build
/// fingerprint, no row is for the old build, or with `--write` a row for
/// the new build is there already or a file does not write.
pub fn run(args: &Args) -> anyhow::Result<bool> {
    let old_bytes = read(&args.old)?;
    let new_bytes = read(&args.new)?;
    let old = Exe::parse(&old_bytes).with_context(|| args.old.display().to_string())?;
    let new = Exe::parse(&new_bytes).with_context(|| args.new.display().to_string())?;
    let (old_fp, new_fp) = fingerprints(&old, &new, args.write)?;
    let mut staged = Staged::default();
    let sections = port_mods(&args.dir, &old, &new, args.write, &mut staged)?;
    if sections.is_empty() {
        bail!("no row is for the old build {}", spell(old_fp));
    }
    let text = report::render(&spell(old_fp), &spell(new_fp), &sections);
    finish(staged, args.report.as_deref(), &text)?;
    Ok(sections
        .iter()
        .flat_map(|s| &s.lines)
        .all(|l| !l.status.needs_human()))
}

/// The build fingerprints of `old` and `new`; with `write`, refused when
/// both are the same build.
///
/// # Errors
///
/// When an executable has no fingerprint, or with `write`, when both are
/// the same build.
pub fn fingerprints(
    old: &Exe<'_>,
    new: &Exe<'_>,
    write: bool,
) -> anyhow::Result<(Fingerprint, Fingerprint)> {
    let old_fp = old
        .fingerprint()
        .context("the old executable has no fingerprint")?;
    let new_fp = new
        .fingerprint()
        .context("the new executable has no fingerprint")?;
    if write && old_fp == new_fp {
        bail!("both executables are the same build; nothing to write");
    }
    Ok((old_fp, new_fp))
}

/// Writes the report to `report` before the staged files, so a report that
/// cannot be written leaves every file as it was; without `report`, prints it
/// once the files are written.
///
/// # Errors
///
/// When the report or a staged file does not write.
pub fn finish(staged: Staged, report: Option<&Path>, text: &str) -> anyhow::Result<()> {
    if let Some(path) = report {
        fs::write(path, text).with_context(|| path.display().to_string())?;
        return staged.commit();
    }
    staged.commit()?;
    let _shown = write!(std::io::stdout(), "{text}");
    Ok(())
}

/// The side the game executable's file name names.
#[must_use]
pub fn side_of_exe(path: &Path) -> Option<Side> {
    let name = path.file_name()?.to_str()?.to_ascii_lowercase();
    match name.as_str() {
        "enshrouded_server.exe" => Some(Side::Server),
        "enshrouded.exe" => Some(Side::Client),
        _ => None,
    }
}

/// The game data crate's spelling of `side`.
#[must_use]
pub const fn gamedata_side(side: Side) -> enshwick_gamedata::Side {
    match side {
        Side::Server => enshwick_gamedata::Side::Server,
        Side::Client => enshwick_gamedata::Side::Client,
    }
}

/// Ports the rows for `old`'s build in every `Cargo.toml` below `dir` to
/// `new`'s build; with `write`, stages each file with its new rows.
fn port_mods(
    dir: &Path,
    old: &Exe<'_>,
    new: &Exe<'_>,
    write: bool,
    staged: &mut Staged,
) -> anyhow::Result<Vec<report::Section>> {
    let (Some(old_fp), Some(new_fp)) = (old.fingerprint(), new.fingerprint()) else {
        return Ok(Vec::new());
    };
    let mut sections = Vec::new();
    for path in manifests(dir) {
        let src = fs::read_to_string(&path).with_context(|| path.display().to_string())?;
        // A file that is not TOML is no package cargo builds: a test fixture.
        if toml::from_str::<toml::Table>(&src).is_err() {
            continue;
        }
        let rows = rows::parse(&src).with_context(|| path.display().to_string())?;
        let file = relative(dir, &path);
        let mut inserts = Vec::new();
        for row in rows.iter().filter(|r| matches(r, old_fp)) {
            if write
                && rows
                    .iter()
                    .any(|r| r.side == row.side && matches(r, new_fp))
            {
                bail!("{file} already has a row for the new build");
            }
            let ported = row::port_row(row, old, new);
            let text = row.render(
                &src,
                &guid_hex(new_fp.guid()),
                new_fp.age(),
                None,
                &ported.edits,
            );
            let title = format!("{:?} row", row.side).to_lowercase();
            if write {
                let (at, lead) = row.insertion(&src);
                inserts.push((at, format!("{lead}{text}")));
            }
            sections.push(report::Section {
                file: file.clone(),
                title,
                source: None,
                lines: ported.lines,
                text: (!write).then_some((text, "toml")),
                unpatterned: Vec::new(),
            });
        }
        if !inserts.is_empty() {
            let mut out = src.clone();
            inserts.sort_by_key(|(at, _)| std::cmp::Reverse(*at));
            for (at, text) in inserts {
                out.insert_str(at, &text);
            }
            staged.replace(path, out, src);
        }
    }
    Ok(sections)
}

/// `path` below `root`, with forward slashes.
#[must_use]
pub fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Every `Cargo.toml` below `dir`, sorted, outside `target`, hidden folders
/// and links, which could lead back up the tree: the manifests that may
/// hold build rows.
fn manifests(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut dirs = vec![dir.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        let mut found: Vec<(PathBuf, fs::FileType)> = entries
            .filter_map(|e| e.ok().and_then(|e| Some((e.path(), e.file_type().ok()?))))
            .collect();
        found.sort_by(|a, b| a.0.cmp(&b.0));
        for (path, kind) in found {
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();
            if kind.is_dir() {
                if name != "target" && !name.starts_with('.') {
                    dirs.push(path);
                }
            } else if kind.is_file() && name == "Cargo.toml" {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

/// Whether `row` is for the build `fingerprint`.
#[must_use]
pub fn matches(row: &Row, fingerprint: Fingerprint) -> bool {
    row.guid == guid_hex(fingerprint.guid()) && row.age == fingerprint.age()
}

/// The build `fingerprint` as `<guid>/<age>`.
#[must_use]
pub fn spell(fingerprint: Fingerprint) -> String {
    format!("{}/{}", guid_hex(fingerprint.guid()), fingerprint.age())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scratch;

    #[test]
    fn a_report_that_cannot_be_written_leaves_the_files_unwritten() {
        let dir = scratch("report");
        let mut staged = Staged::default();
        staged.create(dir.join("new.toml"), "table".into());
        let report = dir.join("missing").join("report.md");
        assert!(finish(staged, Some(&report), "report").is_err());
        assert!(!dir.join("new.toml").exists());
        let mut staged = Staged::default();
        staged.create(dir.join("new.toml"), "table".into());
        let report = dir.join("report.md");
        finish(staged, Some(&report), "report").unwrap();
        assert_eq!(fs::read_to_string(dir.join("new.toml")).unwrap(), "table");
        assert_eq!(fs::read_to_string(report).unwrap(), "report");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_side_comes_from_the_executable_s_name() {
        assert_eq!(
            side_of_exe(Path::new("x/Enshrouded_Server.exe")),
            Some(Side::Server)
        );
        assert_eq!(side_of_exe(Path::new("enshrouded.exe")), Some(Side::Client));
        assert_eq!(side_of_exe(Path::new("game.exe")), None);
    }

    #[test]
    fn every_manifest_of_a_workspace_is_found_but_those_in_target_hidden_folders_and_links() {
        let dir = scratch("manifests");
        for sub in ["", "mods/a", "b", "target/c", ".git/d"] {
            fs::create_dir_all(dir.join(sub)).unwrap();
            fs::write(dir.join(sub).join("Cargo.toml"), "").unwrap();
        }
        // A link back up the tree; where links need rights the test lacks,
        // it checks the rest.
        #[cfg(windows)]
        let _linked = std::os::windows::fs::symlink_dir(&dir, dir.join("b/up"));
        #[cfg(unix)]
        let _linked = std::os::unix::fs::symlink(&dir, dir.join("b/up"));
        let found: Vec<String> = manifests(&dir).iter().map(|p| relative(&dir, p)).collect();
        fs::remove_dir_all(&dir).unwrap();
        assert_eq!(found, ["Cargo.toml", "b/Cargo.toml", "mods/a/Cargo.toml"]);
    }

    /// The rows of every manifest in this repository parse.
    #[test]
    fn the_repository_s_rows_parse() {
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        for path in manifests(&repo) {
            let src = fs::read_to_string(&path).unwrap();
            for row in rows::parse(&src).unwrap() {
                assert!(!row.symbols.is_empty() || !row.constants.is_empty());
            }
        }
    }

    /// A mod with one build row for the server build in `ENSH_SERVER_EXE`
    /// ports it to the same build: its symbol stays, nothing needs a human,
    /// and the new row is in the report; skipped without the executable.
    #[test]
    fn a_mod_with_one_build_row_ports_it() {
        let Some(path) = std::env::var_os("ENSH_SERVER_EXE").map(PathBuf::from) else {
            return;
        };
        let bytes = fs::read(&path).unwrap();
        let exe = Exe::parse(&bytes).unwrap();
        let fp = exe.fingerprint().unwrap();
        let start = exe.functions()[1000].start;
        let dir = scratch("one-row");
        let row = format!(
            "[[package.metadata.enshwick.builds]]\nside = \"server\"\nguid = \"{}\"\nage = {}\n\n\
             [package.metadata.enshwick.builds.symbols]\n\"demo.hook\" = {start:#x}\n",
            guid_hex(fp.guid()),
            fp.age()
        );
        fs::write(
            dir.join("Cargo.toml"),
            format!("[package]\nname = \"demo\"\nversion = \"0.1.0\"\n\n{row}"),
        )
        .unwrap();
        fs::create_dir_all(dir.join("tests/fixtures/broken")).unwrap();
        fs::write(dir.join("tests/fixtures/broken/Cargo.toml"), "[package\n").unwrap();
        let report = dir.join("report.md");
        let settled = run(&Args {
            old: path.clone(),
            new: path,
            dir: dir.clone(),
            write: false,
            report: Some(report.clone()),
        })
        .unwrap();
        let text = fs::read_to_string(&report).unwrap();
        fs::remove_dir_all(&dir).unwrap();
        assert!(settled, "{text}");
        assert!(text.contains("## Cargo.toml: server row"), "{text}");
        assert!(
            text.contains(&format!("| symbol | `demo.hook` | {} |", rows::hex(start))),
            "{text}"
        );
        assert!(text.contains("New row:"), "{text}");
        assert!(
            text.contains(&row),
            "the row ports to its own build as written:\n{text}"
        );
        assert!(!text.contains("broken"), "{text}");
    }
}
