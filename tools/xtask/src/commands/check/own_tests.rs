//! The packages whose tests build alone ([`Package::own_tests`]), outside
//! the workspace's one build, whose features unify the fake host's into the
//! crates below them.

use std::collections::BTreeSet;

use super::Step;
use crate::affected::{Package, Scope};

/// The packages of `scope` whose tests run in a build of their own: those
/// it tests that run on this host, as `host_ok` says.
pub(super) fn own_tests(
    scope: &Scope,
    packages: &[Package],
    host_ok: impl Fn(&String) -> bool,
) -> BTreeSet<String> {
    packages
        .iter()
        .filter(|p| p.own_tests && host_ok(&p.name))
        .filter(|p| match scope {
            Scope::All => true,
            Scope::Some { code, tested, .. } => code.contains(&p.name) || tested.contains(&p.name),
        })
        .map(|p| p.name.clone())
        .collect()
}

/// The test run of each of `own`, a build of its own with its own `test`
/// feature, when it has one, and none of the workspace's.
pub(super) fn own_test_steps(own: &BTreeSet<String>, packages: &[Package]) -> Vec<Step> {
    own.iter()
        .filter_map(|name| packages.iter().find(|p| &p.name == name))
        .map(|package| {
            let step = Step::new(&["nextest", "run", "--no-tests=pass", "--package"])
                .then_owned(std::slice::from_ref(&package.name));
            if package.features.iter().any(|f| f == "test") {
                step.then_owned(&["--features".to_owned(), format!("{}/test", package.name)])
            } else {
                step
            }
        })
        .collect()
}
