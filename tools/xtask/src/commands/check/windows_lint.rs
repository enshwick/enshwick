//! The Windows lints off Windows, where the `cfg(windows)` code builds only
//! through cargo-xwin.

use super::{Part, Step, TEST_FEATURE, WINDOWS_TARGET, features};
use crate::affected::{Package, Scope};

/// The Windows lints of the `parts` named: [`Part::WindowsLint`] the whole
/// workspace with its test features, then [`Part::WindowsFeatures`] each
/// package `scope` checks that has Windows code (`windows_code`, Windows-only
/// packages included) with none and each one of its features alone
/// ([`features`]); the host's feature lints already cover the rest.
pub(super) fn steps(
    scope: &Scope,
    packages: &[Package],
    parts: &[Part],
    windows_code: impl Fn(&Package) -> bool,
    alone: &features::Alone,
) -> Vec<Step> {
    let xwin = ["xwin", "clippy", "--target", WINDOWS_TARGET];
    let mut steps = Vec::new();
    if parts.contains(&Part::WindowsLint) {
        steps.push(
            Step::new(&xwin)
                .then(&["--workspace", "--features", TEST_FEATURE, "--all-targets"])
                .then(&["--", "-D", "warnings"]),
        );
    }
    if parts.contains(&Part::WindowsFeatures) {
        steps.extend(features::steps(scope, packages, windows_code, &xwin, alone));
    }
    steps
}
