//! The tests of the crates that build only for Windows, run off Windows under
//! Wine.
//!
//! `cargo xwin test` builds them and runs every test executable, doctests
//! included, under the host's `wine`. That needs an amd64 Linux host with
//! Wine installed; elsewhere the tests run on Windows.

use std::process::{Command, Stdio};

use crate::commands::check::{Step, WINDOWS_TARGET};
use crate::say;

/// Runs the tests of `packages` under Wine. Without Wine on an amd64 Linux
/// host the step is skipped with a message, and fails only when it was asked
/// for by name (`required`). With `keep_going` every failing test binary
/// runs to its end ([`Step::keep_going`]).
pub(crate) fn run(packages: &[&str], required: bool, keep_going: bool) -> bool {
    if packages.is_empty() {
        return true;
    }
    if cfg!(windows) {
        say("on Windows the Windows tests run with the host tests");
        return true;
    }
    if !native_wine() {
        say(&format!(
            "Wine runs the Windows tests only on an amd64 Linux host with `wine`: the tests of {} are {}",
            packages.join(", "),
            if required { "required" } else { "skipped" }
        ));
        return !required;
    }
    tests(packages).keep_going_when(keep_going).run(false)
}

/// `cargo xwin test` of `packages`, one test at a time: the fake host keeps
/// global state. cargo-xwin runs the executables with `wine` unless a runner
/// is set.
fn tests(packages: &[&str]) -> Step {
    let mut step = Step::new(&["xwin", "test", "--target", WINDOWS_TARGET]);
    for package in packages {
        step = step.then(&["-p", package]);
    }
    step.then(&["--", "--test-threads", "1"])
}

/// Whether Wine runs natively here: an amd64 Linux host with `wine`.
fn native_wine() -> bool {
    cfg!(all(target_os = "linux", target_arch = "x86_64"))
        && Command::new("wine")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_local_run_under_wine_runs_every_failing_test_binary() {
        assert_eq!(
            tests(&["a"]).keep_going_when(true).line(),
            format!(
                "cargo xwin test --target {WINDOWS_TARGET} -p a --no-fail-fast -- --test-threads 1"
            )
        );
        assert!(
            !tests(&["a"])
                .keep_going_when(false)
                .line()
                .contains("--no-fail-fast")
        );
    }
}
