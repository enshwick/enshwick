//! What a check found failing. A local check runs every step that does not
//! need one that failed and lists every failure at its end, since a check
//! that stopped at its first failure took eight runs to show seven problems
//! (2026-11-25); CI stops at the first, which saves runner time.

use super::Step;
use crate::affected::Lane;
use crate::say;

/// The failures of one check, and whether it goes on after one.
#[derive(Debug)]
pub(crate) struct Failures {
    keep_going: bool,
    failed: Vec<String>,
}

impl Failures {
    /// The failures of a check in `lane`: a local check keeps going.
    pub(crate) fn new(lane: Lane) -> Self {
        Self {
            keep_going: lane == Lane::Local,
            failed: Vec::new(),
        }
    }

    pub(crate) const fn keep_going(&self) -> bool {
        self.keep_going
    }

    /// Records `what` as failed unless `passed`; whether the check goes on.
    pub(crate) fn note(&mut self, passed: bool, what: impl Into<String>) -> bool {
        if !passed {
            self.failed.push(what.into());
        }
        !self.stopped()
    }

    /// Whether the check ends here: it failed and does not keep going.
    pub(crate) fn stopped(&self) -> bool {
        !self.keep_going && !self.failed.is_empty()
    }

    pub(crate) fn extend(&mut self, failed: Vec<String>) {
        self.failed.extend(failed);
    }

    /// Lists every failure when there is more than one; whether none failed.
    pub(crate) fn report(&self) -> bool {
        if self.failed.len() > 1 {
            say(&format!("{} steps failed:", self.failed.len()));
            for what in &self.failed {
                say(&format!("  {what}"));
            }
        }
        self.failed.is_empty()
    }
}

/// Runs `steps` in order through `run`, keeping going after a failure when
/// `keep_going`, except past a step that needs the one before it
/// ([`Step::needs_previous`]) when that one failed; the lines of the steps
/// that failed.
pub(crate) fn run_steps(
    steps: &[Step],
    keep_going: bool,
    mut run: impl FnMut(&Step) -> bool,
) -> Vec<String> {
    let mut failed = Vec::new();
    let mut previous = true;
    for step in steps {
        if step.needs_previous && !previous {
            say(&format!("skipped, the step before failed: {}", step.line()));
            continue;
        }
        previous = run(step);
        if !previous {
            failed.push(step.line());
            if !keep_going {
                break;
            }
        }
    }
    failed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn failing(lines: &[&str]) -> impl FnMut(&Step) -> bool {
        move |step| !lines.iter().any(|line| step.line() == *line)
    }

    fn steps() -> Vec<Step> {
        vec![
            Step::new(&["clippy"]),
            Step::new(&["doc"]),
            Step::new(&["links"]).after_previous(),
            Step::new(&["nextest"]),
        ]
    }

    #[test]
    fn a_local_check_reports_two_independent_failures_together() {
        let failed = run_steps(&steps(), true, failing(&["cargo clippy", "cargo nextest"]));
        assert_eq!(failed, ["cargo clippy", "cargo nextest"]);
        let mut failures = Failures::new(Lane::Local);
        assert!(failures.note(false, "tree rules"));
        failures.extend(failed);
        assert_eq!(
            failures.failed,
            ["tree rules", "cargo clippy", "cargo nextest"]
        );
        assert!(!failures.report());
    }

    #[test]
    fn a_ci_check_stops_at_its_first_failure() {
        let failed = run_steps(&steps(), false, failing(&["cargo clippy", "cargo nextest"]));
        assert_eq!(failed, ["cargo clippy"]);
        let mut failures = Failures::new(Lane::Ci);
        assert!(failures.note(true, "tree rules"));
        assert!(!failures.note(false, "C header"));
        assert!(failures.stopped());
    }

    #[test]
    fn a_step_that_needs_the_one_before_skips_when_that_one_failed() {
        let mut ran = Vec::new();
        let failed = run_steps(&steps(), true, |step| {
            ran.push(step.line());
            step.line() != "cargo doc"
        });
        assert_eq!(failed, ["cargo doc"]);
        assert_eq!(ran, ["cargo clippy", "cargo doc", "cargo nextest"]);
    }

    #[test]
    fn keeping_going_asks_cargo_for_every_failure_before_its_passed_arguments() {
        let line = |step: Step| step.keep_going().line();
        assert_eq!(
            line(Step::new(&[
                "clippy",
                "--workspace",
                "--",
                "-D",
                "warnings"
            ])),
            "cargo clippy --workspace --keep-going -- -D warnings"
        );
        assert_eq!(
            line(Step::new(&["xwin", "clippy", "--", "-D", "warnings"])),
            "cargo xwin clippy --keep-going -- -D warnings"
        );
        assert_eq!(
            line(Step::new(&["test", "--doc", "--", "a/"])),
            "cargo test --doc --no-fail-fast -- a/"
        );
        assert_eq!(
            line(Step::new(&["nextest", "run", "-E", "package(a)"])),
            "cargo nextest run -E package(a) --no-fail-fast"
        );
        assert_eq!(line(Step::new(&["doc"])), "cargo doc --keep-going");
        assert_eq!(line(Step::new(&["fmt", "--check"])), "cargo fmt --check");
        assert_eq!(line(Step::tool("ast-grep", &["test"])), "ast-grep test");
    }

    #[test]
    fn a_check_with_no_failure_passes() {
        let mut failures = Failures::new(Lane::Local);
        failures.extend(run_steps(&steps(), true, |_| true));
        assert!(failures.report());
    }
}
