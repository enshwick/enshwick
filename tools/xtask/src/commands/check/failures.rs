//! What a check found failing. A local check runs every step that does not
//! need one that failed and lists every failure at its end, since a check
//! that stopped at its first failure took eight runs to show seven problems
//! (2026-11-25); CI stops at the first, which saves runner time.

use super::Step;
use crate::affected::Lane;
use crate::say;

/// What a run of steps left undone: the lines of the steps that failed, and
/// of those skipped because the step they need failed.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Ran {
    pub(crate) failed: Vec<String>,
    pub(crate) skipped: Vec<String>,
}

impl Ran {
    /// A run that failed as `what`, with nothing skipped.
    pub(crate) fn failed(what: impl Into<String>) -> Self {
        Self {
            failed: vec![what.into()],
            skipped: Vec::new(),
        }
    }
}

/// The failures of one check, and whether it goes on after one.
#[derive(Debug)]
pub(crate) struct Failures {
    keep_going: bool,
    ran: Ran,
}

impl Failures {
    /// The failures of a check in `lane`: a local check keeps going.
    pub(crate) fn new(lane: Lane) -> Self {
        Self {
            keep_going: lane == Lane::Local,
            ran: Ran::default(),
        }
    }

    pub(crate) const fn keep_going(&self) -> bool {
        self.keep_going
    }

    /// Records `what` as failed unless `passed`; whether the check goes on.
    pub(crate) fn note(&mut self, passed: bool, what: impl Into<String>) -> bool {
        if !passed {
            self.ran.failed.push(what.into());
        }
        !self.stopped()
    }

    /// Whether the check ends here: it failed and does not keep going.
    pub(crate) fn stopped(&self) -> bool {
        !self.keep_going && !self.ran.failed.is_empty()
    }

    pub(crate) fn extend(&mut self, ran: Ran) {
        self.ran.failed.extend(ran.failed);
        self.ran.skipped.extend(ran.skipped);
    }

    /// Ends the check on `error` after listing what failed before it, so an
    /// error does not hide those failures.
    pub(crate) fn end_with(&self, error: String) -> String {
        self.report();
        error
    }

    /// Lists every failure and every step skipped for one, so neither far
    /// up the output is lost; whether none failed.
    pub(crate) fn report(&self) -> bool {
        for (heading, lines) in [
            ("failed steps", &self.ran.failed),
            (
                "skipped steps, since a step they need failed",
                &self.ran.skipped,
            ),
        ] {
            if !lines.is_empty() {
                say(&format!("{heading} ({}):", lines.len()));
                for line in lines {
                    say(&format!("  {line}"));
                }
            }
        }
        self.ran.failed.is_empty()
    }
}

/// Runs `steps` in order through `run`, keeping going after a failure when
/// `keep_going`, except past a step that needs the one before it
/// ([`Step::needs_previous`]) when that one failed.
pub(crate) fn run_steps(
    steps: &[Step],
    keep_going: bool,
    mut run: impl FnMut(&Step) -> bool,
) -> Ran {
    let mut ran = Ran::default();
    let mut previous = true;
    for step in steps {
        if step.needs_previous && !previous {
            say(&format!("skipped, the step before failed: {}", step.line()));
            ran.skipped.push(step.line());
            continue;
        }
        previous = run(step);
        if !previous {
            ran.failed.push(step.line());
            if !keep_going {
                break;
            }
        }
    }
    ran
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
        let ran = run_steps(&steps(), true, failing(&["cargo clippy", "cargo nextest"]));
        assert_eq!(ran.failed, ["cargo clippy", "cargo nextest"]);
        let mut failures = Failures::new(Lane::Local);
        assert!(failures.note(false, "tree rules"));
        failures.extend(ran);
        assert_eq!(
            failures.ran.failed,
            ["tree rules", "cargo clippy", "cargo nextest"]
        );
        assert!(!failures.report());
    }

    #[test]
    fn an_error_after_a_failure_keeps_that_failure_and_the_error() {
        let mut failures = Failures::new(Lane::Local);
        assert!(failures.note(false, "the fuzz part"));
        assert_eq!(failures.end_with("no metadata".to_owned()), "no metadata");
        assert_eq!(failures.ran.failed, ["the fuzz part"]);
    }

    #[test]
    fn a_ci_check_stops_at_its_first_failure() {
        let ran = run_steps(&steps(), false, failing(&["cargo clippy", "cargo nextest"]));
        assert_eq!(ran, Ran::failed("cargo clippy"));
        let mut failures = Failures::new(Lane::Ci);
        assert!(failures.note(true, "tree rules"));
        assert!(!failures.note(false, "C header"));
        assert!(failures.stopped());
    }

    #[test]
    fn a_step_that_needs_the_one_before_skips_when_that_one_failed_and_says_so_at_the_end() {
        let mut lines = Vec::new();
        let ran = run_steps(&steps(), true, |step| {
            lines.push(step.line());
            step.line() != "cargo doc"
        });
        assert_eq!(ran.failed, ["cargo doc"]);
        assert_eq!(ran.skipped, ["cargo links"]);
        assert_eq!(lines, ["cargo clippy", "cargo doc", "cargo nextest"]);
        let mut failures = Failures::new(Lane::Local);
        failures.extend(ran);
        assert_eq!(failures.ran.skipped, ["cargo links"]);
        assert!(!failures.report());
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
