//! The compatibility reports a branch leaves stale: CI has no game
//! executables, so a change to the anchors that changes what the loader
//! finds on a kept build passes there unless the reports move with it.

use crate::git;

/// The anchor files, whose change can change every report.
const ANCHORS: &str = "crates/enshwick-gamedata/anchors/";
/// The compatibility reports.
const REPORTS: &str = "crates/enshwick-gamedata/reports/";
/// The commit trailer that says why reports need no change.
const UNCHANGED: &str = "Reports-unchanged";

/// Whether a run of the GitHub `event` gates the reports: a merge queue
/// entry's squash commit drops the branch's trailers, and the pull request's
/// own run already gated the same change.
pub(crate) fn gates(event: Option<&std::ffi::OsStr>) -> bool {
    event.is_none_or(|event| event != "merge_group")
}

/// The `Reports-unchanged` trailer of the newest commit since the merge base
/// with `base` that changes the anchors, in the repository at `root`, so a
/// reason given for an earlier anchor change clears no later one; empty
/// without one, `None` without a merge base.
pub(crate) fn reason(root: &str, base: &str) -> Option<String> {
    let merge_base = git(&["-C", root, "merge-base", "HEAD", base])?;
    git(&[
        "-C",
        root,
        "log",
        "-1",
        &format!("--format=%(trailers:key={UNCHANGED},valueonly)"),
        &format!("{}..HEAD", merge_base.trim()),
        "--",
        ANCHORS,
    ])
}

/// Why the branch is stale: it changes the anchors (`files`, changed since
/// the merge base) but no report, and its newest anchor change gives no
/// `reason`; `None` when it is not.
pub(crate) fn stale(files: &[String], reason: &str) -> Option<String> {
    let anchors: Vec<&str> = files
        .iter()
        .map(String::as_str)
        .filter(|file| file.starts_with(ANCHORS))
        .collect();
    if anchors.is_empty()
        || !reason.trim().is_empty()
        || files.iter().any(|file| file.starts_with(REPORTS))
    {
        return None;
    }
    Some(format!(
        "{} changed but no report in {REPORTS}: regenerate them on the kept executables with `cargo xtask compat refresh`, or say why none changes in a `{UNCHANGED}: <why>` trailer of the commit that changes them",
        anchors.join(", ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| (*name).to_owned()).collect()
    }

    #[test]
    fn an_anchor_change_without_a_report_or_a_reason_is_stale() {
        let routes = files(&["crates/enshwick-gamedata/anchors/routes.toml"]);
        let problem = stale(&routes, "").unwrap();
        assert!(problem.contains("anchors/routes.toml"), "{problem}");
        assert!(
            stale(&routes, "  \n").is_some(),
            "a trailer without a reason"
        );
    }

    #[test]
    fn a_report_change_or_a_reason_clears_an_anchor_change() {
        let both = files(&[
            "crates/enshwick-gamedata/anchors/patterns.toml",
            "crates/enshwick-gamedata/reports/server-ab-1.toml",
        ]);
        assert_eq!(stale(&both, ""), None);
        let patterns = files(&["crates/enshwick-gamedata/anchors/patterns.toml"]);
        assert_eq!(stale(&patterns, "a comment only\n"), None);
    }

    #[test]
    fn a_merge_queue_entry_leaves_the_gate_to_its_pull_request() {
        assert!(!gates(Some("merge_group".as_ref())));
        assert!(gates(Some("pull_request".as_ref())));
        assert!(gates(None));
    }

    #[test]
    fn a_change_outside_the_anchors_is_never_stale() {
        let code = files(&["crates/enshwick-gamedata/src/routes.rs", "README.md"]);
        assert_eq!(stale(&code, ""), None);
    }

    #[test]
    fn only_the_newest_anchor_commit_s_trailer_counts_in_any_case() {
        let root = std::env::temp_dir().join(format!("xtask-reports-{}", std::process::id()));
        if root.exists() {
            std::fs::remove_dir_all(&root).unwrap();
        }
        std::fs::create_dir_all(root.join(ANCHORS)).unwrap();
        let dir = root.to_str().unwrap();
        let run = |args: &[&str]| {
            let mut all = vec![
                "-C",
                dir,
                "-c",
                "user.name=test",
                "-c",
                "user.email=test@example.com",
                "-c",
                "commit.gpgsign=false",
            ];
            all.extend_from_slice(args);
            git(&all).unwrap()
        };
        run(&["init", "--quiet"]);
        run(&["commit", "--quiet", "--allow-empty", "-m", "chore: start"]);
        let base = run(&["rev-parse", "HEAD"]);
        let base = base.trim();
        let anchor = root.join(ANCHORS).join("routes.toml");
        std::fs::write(&anchor, "# a\n").unwrap();
        run(&["add", "."]);
        run(&[
            "commit",
            "--quiet",
            "-m",
            "docs: reword\n\nReports-Unchanged: a comment only",
        ]);
        assert_eq!(reason(dir, base).unwrap().trim(), "a comment only");
        run(&["commit", "--quiet", "--allow-empty", "-m", "chore: later"]);
        assert_eq!(reason(dir, base).unwrap().trim(), "a comment only");
        std::fs::write(&anchor, "# b\n").unwrap();
        run(&["commit", "--quiet", "-am", "feat: route a value"]);
        assert_eq!(reason(dir, base).unwrap().trim(), "");
        std::fs::remove_dir_all(&root).unwrap();
    }
}
