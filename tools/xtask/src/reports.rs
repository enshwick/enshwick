//! The seal of the compatibility reports. CI has no game executables, so it
//! cannot rerun what a report says; each report instead records a digest of
//! the files that decide it together with its own text, and a report left
//! behind by a change to those files, or edited by hand, no longer matches.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::Path;
use std::process::Command;

use sha2::{Digest as _, Sha256};

/// The compatibility reports, from the repository root.
pub(crate) const REPORTS: &str = "crates/enshwick-gamedata/reports";

/// The digest of everything that decides what a report says.
pub(crate) type Inputs = [u8; 32];

/// The files that decide what a report says, as git pathspecs from the
/// repository root: the anchors, the pattern scanner, the code that reads an
/// executable and routes to its entries and fields, the loader's attribute
/// slot and Steam build readers and the tool that writes the report. Files
/// only tests compile are left out; a test module inside a listed file is
/// not. The loader's feature table is spread over its parts, so a test of
/// the compat tool holds it instead.
const INPUTS: &[&str] = &[
    "crates/internal/base/src/code.rs",
    "crates/internal/base/src/code",
    "crates/enshwick-gamedata/anchors",
    "crates/enshwick-gamedata/src/lib.rs",
    "crates/enshwick-gamedata/src/archive.rs",
    "crates/enshwick-gamedata/src/entries.rs",
    "crates/enshwick-gamedata/src/entries",
    "crates/enshwick-gamedata/src/parts.rs",
    "crates/enshwick-gamedata/src/reflect.rs",
    "crates/enshwick-gamedata/src/reflect",
    "crates/enshwick-gamedata/src/resolve.rs",
    "crates/enshwick-gamedata/src/routes.rs",
    "crates/enshwick-gamedata/src/routes",
    "crates/enshwick-gamedata/src/table.rs",
    "crates/enshwick-gamedata/src/table",
    "crates/enshwick-gamedata/src/version.rs",
    "crates/internal/loader/src/attribute_slots.rs",
    "crates/internal/loader/src/builds.rs",
    "tools/xtask/src/compat.rs",
    "tools/xtask/src/compat",
    ":(exclude)crates/enshwick-gamedata/anchors/classes.toml",
    ":(exclude)crates/enshwick-gamedata/src/entries/proofs.rs",
    ":(exclude)crates/enshwick-gamedata/src/table/carry.rs",
    ":(exclude,glob)**/tests.rs",
    ":(exclude,glob)**/tests/**",
];

/// The dependencies whose locked version decides what a report says: the
/// decoder every engine route reads code with.
const LOCKED: &[&str] = &["iced-x86"];

/// The lock file that holds their versions, from the repository root.
const LOCK: &str = "Cargo.lock";

/// The start of the line that holds a report's seal.
const SEAL: &str = "sealed = ";

/// What to do about a report whose seal does not match.
const REFRESH: &str =
    "rewrite the reports with `cargo xtask compat refresh` on the kept executables, never by hand";

/// The input files git lists in the repository at `root` with the
/// `ls-files` options `which`.
pub(crate) fn listed(root: &Path, which: &str) -> Result<BTreeSet<String>, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["ls-files", "-z", which, "--"])
        .args(INPUTS)
        .output()
        .map_err(|e| format!("git: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "git ls-files: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(output
        .stdout
        .split(|&byte| byte == 0)
        .filter(|file| !file.is_empty())
        .map(|file| String::from_utf8_lossy(file).into_owned())
        .collect())
}

/// The name and version of each [`LOCKED`] package in the repository at
/// `root`; none without a lock file.
fn locked(root: &Path) -> Result<Vec<(String, String)>, String> {
    let path = root.join(LOCK);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    let lock: toml::Table = text.parse().map_err(|e| format!("{LOCK}: {e}"))?;
    let mut found: Vec<(String, String)> = lock
        .get("package")
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|package| {
            let name = package.get("name")?.as_str()?;
            let version = package.get("version")?.as_str()?;
            LOCKED
                .contains(&name)
                .then(|| (name.to_owned(), version.to_owned()))
        })
        .collect();
    found.sort();
    Ok(found)
}

/// The digest of the tracked inputs in the repository at `root` as its
/// working tree holds them, line endings as LF, and of the [`LOCKED`]
/// versions; an untracked file, which CI never has, is no input.
///
/// # Errors
///
/// When git fails, an input does not read, or git tracks no input at all,
/// as in a copy of the tree outside its own repository.
pub(crate) fn inputs(root: &Path) -> Result<Inputs, String> {
    let files = listed(root, "--cached")?;
    if files.is_empty() {
        return Err(format!(
            "{}: git tracks none of the files a report's seal covers, so this is not the repository's own checkout",
            root.display()
        ));
    }
    let mut digest = Sha256::new();
    for file in &files {
        let path = root.join(file);
        let text = match std::fs::read(&path) {
            Ok(bytes) => String::from_utf8_lossy(&bytes).replace("\r\n", "\n"),
            // A tracked file deleted in the working tree is no input.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(format!("{}: {e}", path.display())),
        };
        digest.update(file.as_bytes());
        digest.update([0]);
        digest.update((text.len() as u64).to_le_bytes());
        digest.update(text.as_bytes());
    }
    for (name, version) in locked(root)? {
        digest.update(name.as_bytes());
        digest.update([0]);
        digest.update(version.as_bytes());
        digest.update([0]);
    }
    Ok(digest.finalize().into())
}

/// `bytes` in lowercase hexadecimal.
pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut hex, byte| {
        let _w = write!(hex, "{byte:02x}");
        hex
    })
}

/// `report`, without a seal line, sealed for `inputs`: a `sealed` line after
/// its opening comment holds the digest of the inputs and the report's text.
pub(crate) fn sealed(inputs: &Inputs, report: &str) -> String {
    let report = report.replace("\r\n", "\n");
    let digest = Sha256::new()
        .chain_update(inputs)
        .chain_update(report.as_bytes())
        .finalize();
    let at: usize = report
        .split_inclusive('\n')
        .take_while(|line| line.starts_with('#') || line.trim().is_empty())
        .map(str::len)
        .sum();
    format!(
        "{}{SEAL}\"{}\"\n{}",
        &report[..at],
        hex(&digest),
        &report[at..]
    )
}

/// The file name of the build `report` names: its side, then the
/// executable's fingerprint.
fn filed_under(report: &str) -> Option<String> {
    let data: toml::Table = report.parse().ok()?;
    let side = data.get("side")?.as_str()?;
    let executable = data.get("build")?.as_str()?.rsplit_once("executable ")?.1;
    Some(format!("{side}-{}.toml", executable.replace('/', "-")))
}

/// Each report in the repository at `root`, but those named in `skip`, whose
/// seal does not match its text and the current inputs: a report a change to
/// what decides it left behind, or one edited by hand, copied or renamed.
pub(crate) fn stale(root: &Path, skip: &[String]) -> Vec<String> {
    let dir = root.join(REPORTS);
    let read = std::fs::read_dir(&dir).and_then(|entries| {
        entries
            .map(|entry| entry.map(|e| e.path()))
            .collect::<Result<Vec<_>, _>>()
    });
    let mut files = match read {
        Ok(files) => files,
        Err(e) => return vec![format!("{REPORTS}: {e}")],
    };
    files.retain(|file| file.extension().is_some_and(|ext| ext == "toml"));
    files.sort();
    let inputs = match inputs(root) {
        Ok(inputs) => inputs,
        Err(e) => return vec![e],
    };
    let mut problems = Vec::new();
    for file in files {
        let name = file.file_name().unwrap_or_default().to_string_lossy();
        if skip.iter().any(|skipped| *skipped == name) {
            continue;
        }
        let text = match std::fs::read_to_string(&file) {
            Ok(text) => text.replace("\r\n", "\n"),
            Err(e) => {
                problems.push(format!("{name}: {e}"));
                continue;
            }
        };
        let mut unsealed = String::with_capacity(text.len());
        let mut seals = 0;
        for line in text.split_inclusive('\n') {
            if line.starts_with(SEAL) {
                seals += 1;
            } else {
                unsealed.push_str(line);
            }
        }
        if seals != 1 || sealed(&inputs, &unsealed) != text {
            problems.push(format!(
                "{name} is not what the current anchors and route code give, or was edited: {REFRESH}"
            ));
        } else if filed_under(&unsealed).is_none_or(|want| want != name) {
            problems.push(format!(
                "{name} is not filed under the build it names: {REFRESH}"
            ));
        }
    }
    problems
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A report as the test repositories hold it, before its seal.
    pub(crate) const REPORT: &str = "# A report.\n\nbuild = \"server, executable ab/1\"\nside = \"server\"\nrouted = 1\nmissing = []\n";

    /// The route file every test repository holds.
    pub(crate) const ROUTE: &str = "crates/enshwick-gamedata/src/routes/find.rs";

    /// A git repository of its own for this test, holding a route file and
    /// a report sealed for it.
    pub(crate) fn repository(name: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("xtask-seal-{name}-{}", std::process::id()));
        if root.exists() {
            std::fs::remove_dir_all(&root).unwrap();
        }
        std::fs::create_dir_all(root.join(REPORTS)).unwrap();
        assert!(
            Command::new("git")
                .arg("-C")
                .arg(&root)
                .args(["init", "--quiet"])
                .status()
                .unwrap()
                .success()
        );
        write_file(&root, ROUTE, "fn find() {}\n");
        let report = sealed(&inputs(&root).unwrap(), REPORT);
        write_file(&root, &format!("{REPORTS}/server-ab-1.toml"), &report);
        root
    }

    /// Writes `file` in the repository at `root` and tracks it.
    pub(crate) fn write_file(root: &Path, file: &str, text: &str) {
        let path = root.join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
        assert!(
            Command::new("git")
                .arg("-C")
                .arg(root)
                .args(["add", "--", file])
                .status()
                .unwrap()
                .success()
        );
    }

    /// A lock file that pins `iced` and `memchr`.
    fn lock(iced: &str, memchr: &str) -> String {
        format!(
            "version = 4\n\n[[package]]\nname = \"iced-x86\"\nversion = \"{iced}\"\n\n[[package]]\nname = \"memchr\"\nversion = \"{memchr}\"\n"
        )
    }

    #[test]
    fn a_tracked_input_deleted_from_the_tree_leaves_the_report_stale() {
        let root = repository("deleted");
        write_file(&root, "crates/enshwick-gamedata/src/routes/code.rs", "\n");
        let report = sealed(&inputs(&root).unwrap(), REPORT);
        write_file(&root, &format!("{REPORTS}/server-ab-1.toml"), &report);
        std::fs::remove_file(root.join(ROUTE)).unwrap();
        let problems = stale(&root, &[]);
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].starts_with("server-ab-1.toml"), "{problems:?}");
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn an_untracked_file_among_the_inputs_leaves_the_reports_sealed() {
        let root = repository("untracked");
        let scratch = root.join("crates/enshwick-gamedata/src/routes/find.rs.orig");
        std::fs::write(scratch, "fn old() {}\n").unwrap();
        assert_eq!(stale(&root, &[]), Vec::<String>::new());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_tree_where_git_tracks_no_input_fails_rather_than_seal_nothing() {
        let root = repository("empty");
        assert!(
            Command::new("git")
                .arg("-C")
                .arg(&root)
                .args(["rm", "--quiet", "--cached", "--", ROUTE])
                .status()
                .unwrap()
                .success()
        );
        let problems = stale(&root, &[]);
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].contains("tracks none"), "{problems:?}");
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_report_sealed_for_the_current_inputs_passes() {
        let root = repository("current");
        assert_eq!(stale(&root, &[]), Vec::<String>::new());
        let report = std::fs::read_to_string(root.join(REPORTS).join("server-ab-1.toml")).unwrap();
        assert!(report.starts_with("# A report.\n\nsealed = \""), "{report}");
        let data: toml::Table = report.parse().unwrap();
        assert_eq!(data["sealed"].as_str().unwrap().len(), 64);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_report_copied_to_another_build_s_file_fails() {
        let root = repository("copied");
        let dir = root.join(REPORTS);
        std::fs::copy(dir.join("server-ab-1.toml"), dir.join("server-cd-1.toml")).unwrap();
        let problems = stale(&root, &[]);
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(
            problems[0].starts_with("server-cd-1.toml is not filed"),
            "{problems:?}"
        );
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_route_code_change_without_a_new_report_fails() {
        let root = repository("route");
        write_file(&root, ROUTE, "fn find() { todo!() }\n");
        let problems = stale(&root, &[]);
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].contains("server-ab-1.toml"), "{problems:?}");
        assert!(
            problems[0].contains("cargo xtask compat refresh"),
            "{problems:?}"
        );
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_pattern_scanner_change_without_a_new_report_fails() {
        let root = repository("scanner");
        write_file(
            &root,
            "crates/internal/base/src/code/each.rs",
            "fn each() {}\n",
        );
        assert_eq!(stale(&root, &[]).len(), 1);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn only_a_new_decoder_version_leaves_the_reports_stale() {
        let root = repository("lock");
        write_file(&root, LOCK, &lock("1.21.0", "2.7.0"));
        let report = sealed(&inputs(&root).unwrap(), REPORT);
        write_file(&root, &format!("{REPORTS}/server-ab-1.toml"), &report);
        write_file(&root, LOCK, &lock("1.21.0", "2.8.0"));
        assert_eq!(stale(&root, &[]), Vec::<String>::new());
        write_file(&root, LOCK, &lock("1.22.0", "2.8.0"));
        assert_eq!(stale(&root, &[]).len(), 1);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_new_anchor_file_without_a_new_report_fails() {
        let root = repository("anchor");
        write_file(
            &root,
            "crates/enshwick-gamedata/anchors/routes.toml",
            "[x]\n",
        );
        assert_eq!(stale(&root, &[]).len(), 1);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_hand_edited_or_unsealed_report_fails() {
        let root = repository("edited");
        let file = root.join(REPORTS).join("server-ab-1.toml");
        let report = std::fs::read_to_string(&file).unwrap();
        std::fs::write(&file, report.replace("routed = 1", "routed = 2")).unwrap();
        assert_eq!(stale(&root, &[]).len(), 1);
        std::fs::write(&file, REPORT).unwrap();
        assert_eq!(stale(&root, &[]).len(), 1, "a report without a seal");
        std::fs::write(
            &file,
            report.replace("# A report.\n", "# A report.\nsealed = \"0\"\n"),
        )
        .unwrap();
        assert_eq!(stale(&root, &[]).len(), 1, "a report with two seals");
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn tests_and_files_outside_the_inputs_leave_the_reports_sealed() {
        let root = repository("outside");
        for file in [
            "crates/enshwick-gamedata/src/routes/tests.rs",
            "crates/enshwick-gamedata/src/routes/tests/builds.rs",
            "crates/enshwick-gamedata/src/table/carry.rs",
            "crates/enshwick-gamedata/src/entries/proofs.rs",
            "crates/enshwick-gamedata/anchors/classes.toml",
            "crates/enshwick-gamedata/src/typed.rs",
            "README.md",
        ] {
            write_file(&root, file, "\n");
        }
        assert_eq!(stale(&root, &[]), Vec::<String>::new());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn line_endings_do_not_break_a_seal() {
        let root = repository("crlf");
        write_file(&root, ROUTE, "fn find() {}\r\n");
        let file = root.join(REPORTS).join("server-ab-1.toml");
        let report = std::fs::read_to_string(&file).unwrap();
        std::fs::write(&file, report.replace('\n', "\r\n")).unwrap();
        assert_eq!(stale(&root, &[]), Vec::<String>::new());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn every_input_path_names_a_tracked_file() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        for spec in INPUTS.iter().filter(|spec| !spec.starts_with(':')) {
            let found = Command::new("git")
                .arg("-C")
                .arg(&root)
                .args(["ls-files", "--error-unmatch", "--", spec])
                .output()
                .unwrap();
            assert!(found.status.success(), "{spec} names no tracked file");
        }
    }
}
