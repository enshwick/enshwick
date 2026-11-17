//! The lints of each package with none of its features and with each alone.
//! The workspace build lints each package with the union of the features its
//! members and `host_args` turn on, so code that one feature alone leaves
//! unused, or that builds only beside another feature, passes it and fails
//! the first build that turns on that feature by itself. Only
//! the library and binaries: the dev-dependencies of the other targets turn
//! the features back on and hide the gap, and the workspace's clippy lints
//! those targets. Off Windows the Windows lint runs them again for the
//! Windows target, where the `cfg(windows)` code builds.

use super::Step;
use crate::affected::{Package, Scope};

/// The runs of `clippy` (its cargo words, such as `["clippy"]` or the
/// cross-compiling `["xwin", "clippy", "--target", ..]`) over the packages
/// `scope` checks in full that have features and that `builds` keeps: one
/// without any feature, then one for each feature alone.
pub(super) fn steps(
    scope: &Scope,
    packages: &[Package],
    builds: impl Fn(&String) -> bool,
    clippy: &[&str],
) -> Vec<Step> {
    packages
        .iter()
        .filter(|package| !package.features.is_empty() && builds(&package.name))
        .filter(|package| match scope {
            Scope::All => true,
            Scope::Some { code, .. } => code.contains(&package.name),
        })
        .flat_map(|package| {
            let alone = package.features.iter().map(Some);
            std::iter::once(None).chain(alone).map(|feature| {
                let mut step =
                    Step::new(clippy).then(&["--package", &package.name, "--no-default-features"]);
                if let Some(feature) = feature {
                    step = step.then(&["--features", feature]);
                }
                step.then(&["--", "-D", "warnings"])
            })
        })
        .collect()
}
