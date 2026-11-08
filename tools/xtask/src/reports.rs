//! The compatibility reports a branch leaves stale: CI has no game
//! executables, so a change to the anchors that changes what the loader
//! finds on a kept build passes there unless the reports move with it.

/// The anchor files, whose change can change every report.
const ANCHORS: &str = "crates/enshwick-gamedata/anchors/";
/// The compatibility reports.
const REPORTS: &str = "crates/enshwick-gamedata/reports/";
/// The commit trailer that says why reports need no change.
const UNCHANGED: &str = "Reports-unchanged:";

/// Why the branch is stale: it changes the anchors (`files`, changed since
/// the merge base) but no report, and none of its commit `messages` carries
/// a `Reports-unchanged: <why>` trailer; `None` when it is not.
pub(crate) fn stale(files: &[String], messages: &str) -> Option<String> {
    let anchors: Vec<&str> = files
        .iter()
        .map(String::as_str)
        .filter(|file| file.starts_with(ANCHORS))
        .collect();
    let explained = messages.lines().any(|line| {
        line.strip_prefix(UNCHANGED)
            .is_some_and(|why| !why.trim().is_empty())
    });
    if anchors.is_empty() || explained || files.iter().any(|file| file.starts_with(REPORTS)) {
        return None;
    }
    Some(format!(
        "{} changed but no report in {REPORTS}: regenerate them on the kept executables with `cargo xtask compat refresh`, or say why none changes in a `{UNCHANGED} <why>` commit trailer",
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
        let problem = stale(&routes, "feat(loader): route a value\n").unwrap();
        assert!(problem.contains("anchors/routes.toml"), "{problem}");
        assert!(
            stale(
                &routes,
                "feat(loader): route a value\n\nReports-unchanged:  \n"
            )
            .is_some(),
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
        let message = "fix(loader): reword a pattern note\n\nReports-unchanged: a comment only\n";
        assert_eq!(stale(&patterns, message), None);
    }

    #[test]
    fn a_change_outside_the_anchors_is_never_stale() {
        let code = files(&["crates/enshwick-gamedata/src/routes.rs", "README.md"]);
        assert_eq!(stale(&code, ""), None);
    }
}
