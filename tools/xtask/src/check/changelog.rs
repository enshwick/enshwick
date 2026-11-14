//! The changelogs and change files of the workspace's packages: a pull
//! request adds its entries as a file in its package's `changes/` folder,
//! so a change written under a changelog's Unreleased section by hand, a
//! change file that does not read and a `changes/` folder without a
//! changelog beside it are problems.

use std::path::Path;

use cargo_enshwick::changelog::{CHANGELOG, CHANGES, Fragment, hand_edited};

use crate::affected::Package;

/// The problems of every package's changelog and change files, one line
/// each, naming the file from the repository `root`.
pub(crate) fn check(root: &Path, packages: &[Package]) -> Vec<String> {
    let mut problems = Vec::new();
    for package in packages {
        let dir = root.join(&package.dir);
        let shown = |name: &str| format!("{}/{name}", package.dir);
        let changelog = match std::fs::read_to_string(dir.join(CHANGELOG)) {
            Ok(text) => Some(text),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => {
                problems.push(format!("{}: {e}", shown(CHANGELOG)));
                continue;
            }
        };
        if changelog.as_deref().is_some_and(hand_edited) {
            problems.push(format!(
                "{}: changes under Unreleased; a pull request adds them as a file in {}/{CHANGES}/",
                shown(CHANGELOG),
                package.dir
            ));
        }
        let files = match fragments(&dir) {
            Ok(files) => files,
            Err(why) => {
                problems.push(format!("{}: {why}", shown(CHANGES)));
                continue;
            }
        };
        if !files.is_empty() && changelog.is_none() {
            problems.push(format!(
                "{}: change files, but no {CHANGELOG} beside them to join into",
                shown(CHANGES)
            ));
        }
        for (name, text) in files {
            let shown = shown(&format!("{CHANGES}/{name}"));
            if Path::new(&name)
                .extension()
                .is_none_or(|ext| !ext.eq_ignore_ascii_case("md"))
            {
                problems.push(format!("{shown}: a change file is Markdown, `<name>.md`"));
            } else if let Err(why) = Fragment::parse(&text) {
                problems.push(format!("{shown}: {why}"));
            }
        }
    }
    problems
}

/// The change files of the package in `dir`, by name and sorted, each with
/// its text; none without a `changes/` folder.
///
/// # Errors
///
/// When the folder or a file in it does not read.
pub(crate) fn fragments(dir: &Path) -> Result<Vec<(String, String)>, String> {
    let folder = dir.join(CHANGES);
    let entries = match std::fs::read_dir(&folder) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.to_string()),
    };
    let mut files = Vec::new();
    for entry in entries {
        let path = entry.map_err(|e| e.to_string())?.path();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_owned();
        let text = std::fs::read_to_string(&path).map_err(|e| format!("{name}: {e}"))?;
        files.push((name, text));
    }
    files.sort();
    Ok(files)
}

/// Whether `path`, from the repository root with forward slashes as git
/// names it, is a change file: Markdown in a package's `changes/` folder.
pub(crate) fn is_change_file(path: &str) -> bool {
    let path = Path::new(path);
    path.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
        && path
            .parent()
            .and_then(Path::file_name)
            .is_some_and(|dir| dir == CHANGES)
}

/// The change files a branch edits, from `git diff --name-status` against
/// its merge base, one problem each: a pull request adds change files of
/// its own and never edits another's, so two pull requests never meet in
/// one file. Removing them, as the release pull request does, is no edit.
pub(crate) fn edited(name_status: &str) -> Vec<String> {
    name_status
        .lines()
        .filter_map(|line| {
            let (status, path) = line.split_once('\t')?;
            (status.starts_with('M') && is_change_file(path)).then(|| {
                format!(
                    "{path}: edits a change file of another pull request; add one of your own, named after your branch"
                )
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn package(dir: &str) -> Package {
        Package {
            name: dir.to_owned(),
            version: "1.0.0".to_owned(),
            dir: dir.to_owned(),
            deps: Vec::new(),
            lib: None,
            windows_only: false,
            windows_tests: false,
            stable_build: false,
        }
    }

    #[test]
    fn a_branch_that_edits_a_change_file_on_its_base_is_a_problem_and_one_that_adds_or_removes_is_not()
     {
        let status = "M\tcrates/enshwick/changes/earlier.md\n\
                      A\tcrates/enshwick/changes/my-branch.md\n\
                      D\tcrates/internal/base/changes/other.md\n\
                      M\tcrates/enshwick/CHANGELOG.md\n\
                      M\tcrates/runtime/loader/src/content/changes/items.rs\n\
                      R100\tcrates/enshwick/changes/a.md\tcrates/enshwick/changes/b.md\n";
        assert_eq!(
            edited(status),
            [
                "crates/enshwick/changes/earlier.md: edits a change file of another pull request; add one of your own, named after your branch"
            ]
        );
        assert!(is_change_file("changes/a.md"));
        assert!(!is_change_file("book/changes.md"));
        assert!(!is_change_file("a.md"));
    }

    #[test]
    fn hand_edits_malformed_and_orphaned_change_files_are_problems() {
        let root = std::env::temp_dir().join(format!("xtask-changes-{}", std::process::id()));
        drop(std::fs::remove_dir_all(&root));
        let write = |path: &str, text: &str| {
            let path = root.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        };
        write("a/CHANGELOG.md", "## [Unreleased]\n\n- By hand.\n");
        write("a/changes/good.md", "### Added\n\n- A call.\n");
        write("a/changes/bad.md", "- No heading.\n");
        write("a/changes/notes.txt", "### Added\n\n- A call.\n");
        write("b/changes/orphan.md", "### Fixed\n\n- A bug.\n");
        write(
            "c/CHANGELOG.md",
            "## [Unreleased]\n\n## [1.0.0]\n\n- First.\n",
        );
        let problems = check(&root, &[package("a"), package("b"), package("c")]);
        drop(std::fs::remove_dir_all(&root));
        assert_eq!(
            problems,
            [
                "a/CHANGELOG.md: changes under Unreleased; a pull request adds them as a file in a/changes/",
                "a/changes/bad.md: line 1: an entry stands under a heading",
                "a/changes/notes.txt: a change file is Markdown, `<name>.md`",
                "b/changes: change files, but no CHANGELOG.md beside them to join into",
            ]
        );
    }
}
