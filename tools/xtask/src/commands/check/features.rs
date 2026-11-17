//! The lints of each package with none of its features and with each alone.
//! The workspace build lints each package with the union of the features its
//! members and `host_args` turn on, so code that one feature alone leaves
//! unused, or that builds only beside another feature, passes it and fails
//! the first build that turns on that feature by itself. Only
//! the library and binaries: the dev-dependencies of the other targets turn
//! the features back on and hide the gap, and the workspace's clippy lints
//! those targets.

use super::Step;
use crate::affected::{Package, Scope};

/// The clippy runs of the packages `scope` checks in full that have features
/// and build on this host (`host_ok`): one without any feature, then one
/// for each feature alone.
pub(super) fn steps(
    scope: &Scope,
    packages: &[Package],
    host_ok: impl Fn(&String) -> bool,
) -> Vec<Step> {
    packages
        .iter()
        .filter(|package| !package.features.is_empty() && host_ok(&package.name))
        .filter(|package| match scope {
            Scope::All => true,
            Scope::Some { code, .. } => code.contains(&package.name),
        })
        .flat_map(|package| {
            let alone = package.features.iter().map(Some);
            std::iter::once(None).chain(alone).map(|feature| {
                let mut step = Step::new(&[
                    "clippy",
                    "--package",
                    &package.name,
                    "--no-default-features",
                ]);
                if let Some(feature) = feature {
                    step = step.then(&["--features", feature]);
                }
                step.then(&["--", "-D", "warnings"])
            })
        })
        .collect()
}
