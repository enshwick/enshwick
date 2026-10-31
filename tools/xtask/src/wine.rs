//! The tests of the crates that build only for Windows, run off Windows under
//! Wine.
//!
//! `cargo xwin test` builds them and runs every test executable, doctests
//! included, under the host's `wine`. That needs an amd64 Linux host with
//! Wine installed, as on CI; elsewhere the tests run on Windows.

use std::process::{Command, Stdio};

use crate::{Step, WINDOWS_TARGET, say};

/// Runs the tests of `packages` under Wine. Without Wine on an amd64 Linux
/// host the step is skipped with a message, and fails only when it was asked
/// for by name (`required`).
pub(crate) fn run(packages: &[&str], required: bool) -> bool {
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
    tests(packages).run(false)
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
