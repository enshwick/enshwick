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

/// The files that decide what a report says, as git pathspecs from the
/// repository root: the anchors, the code that reads an executable and routes
/// to its entries, slots and fields, the Steam build reader and the tool that
/// writes the report, tests left out. The loader's feature table is spread
/// over its parts, so a test of the compat tool holds it instead.
const INPUTS: &[&str] = &[
    "crates/enshwick-gamedata/anchors",
    "crates/enshwick-gamedata/src/lib.rs",
    "crates/enshwick-gamedata/src/archive.rs",
    "crates/enshwick-gamedata/src/classes.rs",
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
    "crates/internal/loader/src/builds.rs",
    "tools/xtask/src/compat.rs",
    "tools/xtask/src/compat",
    ":(exclude,glob)**/tests.rs",
    ":(exclude,glob)**/tests/**",
];

/// The start of the line that holds a report's seal.
const SEAL: &str = "sealed = ";

/// What to do about a report whose seal does not match.
const REFRESH: &str =
    "rewrite the reports with `cargo xtask compat refresh` on the kept executables, never by hand";

/// The digest of the inputs in the repository at `root` as its working tree
/// holds them, untracked files included and line endings as LF.
pub(crate) fn inputs(root: &Path) -> Result<Sha256, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args([
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
            "--",
        ])
        .args(INPUTS)
        .output()
        .map_err(|e| format!("git: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "git ls-files: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let files: BTreeSet<&[u8]> = output
        .stdout
        .split(|&byte| byte == 0)
        .filter(|file| !file.is_empty())
        .collect();
    let mut digest = Sha256::new();
    for file in files {
        let name = String::from_utf8_lossy(file);
        let path = root.join(name.as_ref());
        let text = match std::fs::read(&path) {
            Ok(bytes) => String::from_utf8_lossy(&bytes).replace("\r\n", "\n"),
            // A tracked file deleted in the working tree is no input.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(format!("{}: {e}", path.display())),
        };
        digest.update(file);
        digest.update([0]);
        digest.update((text.len() as u64).to_le_bytes());
        digest.update(text.as_bytes());
    }
    Ok(digest)
}

/// `report`, without a seal line, sealed for `inputs`: a `sealed` line after
/// its opening comment holds the digest of the inputs and the report's text.
pub(crate) fn sealed(inputs: &Sha256, report: &str) -> String {
    let report = report.replace("\r\n", "\n");
    let mut digest = inputs.clone();
    digest.update(report.as_bytes());
    let hex = digest
        .finalize()
        .iter()
        .fold(String::new(), |mut hex, byte| {
            let _w = write!(hex, "{byte:02x}");
            hex
        });
    let at: usize = report
        .split_inclusive('\n')
        .take_while(|line| line.starts_with('#') || line.trim().is_empty())
        .map(str::len)
        .sum();
    format!("{}{SEAL}\"{hex}\"\n{}", &report[..at], &report[at..])
}

/// Each report in the repository at `root` whose seal does not match its
/// text and the current inputs: a report a change to what decides it left
/// behind, or one edited by hand.
pub(crate) fn check(root: &Path) -> Vec<String> {
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
        }
    }
    problems
}

#[cfg(test)]
mod tests {
    use super::*;

    const REPORT: &str = "# A report.\n\nbuild = \"server, executable ab/1\"\nside = \"server\"\nrouted = 1\nmissing = []\n";

    /// A git repository of its own for this test, holding a route file and
    /// a report sealed for it.
    fn repository(name: &str) -> std::path::PathBuf {
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
        write(
            &root,
            "crates/enshwick-gamedata/src/routes/find.rs",
            "fn find() {}\n",
        );
        let report = sealed(&inputs(&root).unwrap(), REPORT);
        write(&root, &format!("{REPORTS}/server-ab-1.toml"), &report);
        root
    }

    fn write(root: &Path, file: &str, text: &str) {
        let path = root.join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    #[test]
    fn a_report_sealed_for_the_current_inputs_passes() {
        let root = repository("current");
        assert_eq!(check(&root), Vec::<String>::new());
        let report = std::fs::read_to_string(root.join(REPORTS).join("server-ab-1.toml")).unwrap();
        assert!(report.starts_with("# A report.\n\nsealed = \""), "{report}");
        let data: toml::Table = report.parse().unwrap();
        assert_eq!(data["sealed"].as_str().unwrap().len(), 64);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_route_code_change_without_a_new_report_fails() {
        let root = repository("route");
        write(
            &root,
            "crates/enshwick-gamedata/src/routes/find.rs",
            "fn find() { todo!() }\n",
        );
        let problems = check(&root);
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].contains("server-ab-1.toml"), "{problems:?}");
        assert!(
            problems[0].contains("cargo xtask compat refresh"),
            "{problems:?}"
        );
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_new_anchor_file_without_a_new_report_fails() {
        let root = repository("anchor");
        write(
            &root,
            "crates/enshwick-gamedata/anchors/routes.toml",
            "[x]\n",
        );
        assert_eq!(check(&root).len(), 1);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_hand_edited_or_unsealed_report_fails() {
        let root = repository("edited");
        let file = root.join(REPORTS).join("server-ab-1.toml");
        let report = std::fs::read_to_string(&file).unwrap();
        std::fs::write(&file, report.replace("routed = 1", "routed = 2")).unwrap();
        assert_eq!(check(&root).len(), 1);
        std::fs::write(&file, REPORT).unwrap();
        assert_eq!(check(&root).len(), 1, "a report without a seal");
        std::fs::write(
            &file,
            report.replace("# A report.\n", "# A report.\nsealed = \"0\"\n"),
        )
        .unwrap();
        assert_eq!(check(&root).len(), 1, "a report with two seals");
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn tests_and_files_outside_the_inputs_leave_the_reports_sealed() {
        let root = repository("outside");
        write(
            &root,
            "crates/enshwick-gamedata/src/routes/tests.rs",
            "#[test]\nfn t() {}\n",
        );
        write(
            &root,
            "crates/enshwick-gamedata/src/routes/tests/builds.rs",
            "\n",
        );
        write(&root, "crates/enshwick-gamedata/src/typed.rs", "\n");
        write(&root, "README.md", "# Readme\n");
        assert_eq!(check(&root), Vec::<String>::new());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn line_endings_do_not_break_a_seal() {
        let root = repository("crlf");
        write(
            &root,
            "crates/enshwick-gamedata/src/routes/find.rs",
            "fn find() {}\r\n",
        );
        let file = root.join(REPORTS).join("server-ab-1.toml");
        let report = std::fs::read_to_string(&file).unwrap();
        std::fs::write(&file, report.replace('\n', "\r\n")).unwrap();
        assert_eq!(check(&root), Vec::<String>::new());
        std::fs::remove_dir_all(&root).unwrap();
    }
}
