//! The parts of the check, which CI runs as parallel jobs, and which of them
//! have a step for a change.

use clap::ValueEnum as _;

use super::{FACADE, Package, Scope, features, touches, wide};

/// A part of the check, so CI can run the parts as parallel jobs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub(crate) enum Part {
    /// fmt with `--all`, clippy, nextest and cargo-deny for the host, and the
    /// repository's own checks.
    Host,
    /// clippy of each checked package with none and each one of its
    /// features alone, for the host ([`features`]).
    Features,
    /// The doctests and the workspace rustdoc, for the host.
    Docs,
    /// The API crate's stable-surface checks ([`stable_api_steps`](super::stable_api_steps)): a
    /// build of its own without `unstable`, which shares nothing with the
    /// other parts' builds.
    StableApi,
    /// clippy of the workspace for the Windows target through cargo-xwin,
    /// off Windows ([`windows_lint::steps`](super::windows_lint::steps)).
    WindowsLint,
    /// clippy for the Windows target through cargo-xwin, off Windows, of
    /// each checked package with Windows code with none and each one of its
    /// features alone ([`windows_lint::steps`](super::windows_lint::steps)); a part of its own, so CI runs
    /// it beside the workspace's Windows lint.
    WindowsFeatures,
    /// The tests of the changed packages that run only on Windows, under
    /// Wine on an amd64 Linux host.
    WindowsTests,
    /// Short runs of the fuzz targets the change reaches ([`fuzz`](super::fuzz)); only
    /// when named, since it needs the nightly compiler.
    Fuzz,
    /// The trybuild test ([`TRYBUILD`](super::TRYBUILD)), when the API crate's tests are in
    /// the change's scope; a run that finds no such test fails, so a moved
    /// test does not go untested. The host part never runs it.
    Trybuild,
    /// cargo-semver-checks for the packages `--semver` names.
    Semver,
}

impl Part {
    /// The parts that run when none is named: the host check, the lints of
    /// each feature alone, the docs, the stable surface, the Windows lints
    /// of the workspace and of each feature alone, the trybuild test and
    /// semver-checks, and the Windows tests with
    /// `--all` or `--windows`.
    pub(super) fn every(windows_tests: bool) -> Vec<Self> {
        let mut every = vec![
            Self::Host,
            Self::Features,
            Self::Docs,
            Self::StableApi,
            Self::WindowsLint,
            Self::WindowsFeatures,
            Self::Trybuild,
            Self::Semver,
        ];
        if windows_tests {
            every.push(Self::WindowsTests);
        }
        every
    }

    /// Whether the part has a step for the change `scope`: the trybuild test
    /// only where the change reaches the facade and is not [`wide`], the
    /// stable surface where [`stable_api_due`] says, the Windows feature lints only as `windows_features`
    /// ([`windows_features_due`]) says; the other parts find their own work.
    pub(super) fn due(self, scope: &Scope, windows_features: bool) -> bool {
        match self {
            Self::Trybuild => trybuild_due(scope),
            Self::StableApi => stable_api_due(scope),
            Self::WindowsFeatures => windows_features,
            Self::Host
            | Self::Features
            | Self::Docs
            | Self::WindowsLint
            | Self::WindowsTests
            | Self::Fuzz
            | Self::Semver => true,
        }
    }
}

/// Whether the trybuild test runs for `scope`: the change reaches the facade's
/// code and is not [`wide`].
pub(super) fn trybuild_due(scope: &Scope) -> bool {
    touches(scope, FACADE) && !wide(scope)
}

/// Whether the Windows feature lints have a step for the change `scope` to
/// `packages`: it checks a package with features and Windows code
/// (`windows_code`), the packages [`windows_lint::steps`](super::windows_lint::steps) lints off Windows.
/// On Windows the host's own lints cover that code, so the part plans no step
/// there whatever this says.
pub(super) fn windows_features_due(
    scope: &Scope,
    packages: &[Package],
    windows_code: impl Fn(&Package) -> bool,
) -> bool {
    features::linted(scope, packages, windows_code)
        .next()
        .is_some()
}

/// The names of the `parts` that have a step for the change `scope`
/// ([`Part::due`]).
pub(super) fn due_parts(parts: &[Part], scope: &Scope, windows_features: bool) -> Vec<String> {
    parts
        .iter()
        .filter(|part| part.due(scope, windows_features))
        .filter_map(|part| Some(part.to_possible_value()?.get_name().to_owned()))
        .collect()
}

/// Whether `scope` needs the facade's stable surface checked: its examples,
/// the book's among them, change with its documents as well as its code,
/// and a [`wide`] change that tests the facade can break its lints and
/// rustdoc without `unstable` from a crate below it.
pub(super) fn stable_api_due(scope: &Scope) -> bool {
    touches(scope, FACADE)
        || matches!(scope, Scope::Some { docs, tested, wide, .. }
            if docs.contains(FACADE) || (*wide && tested.contains(FACADE)))
}

/// Whether a check of `parts` checks the stable surface of `scope` but
/// leaves its examples to CI because the change is [`wide`].
pub(super) fn stable_examples_left_to_ci(scope: &Scope, parts: &[Part]) -> bool {
    wide(scope) && parts.contains(&Part::StableApi) && stable_api_due(scope)
}

/// Whether a check of `parts` names the stable surface but `scope` does not
/// reach it, so the part passes without a step.
pub(super) fn stable_api_out_of_scope(scope: &Scope, parts: &[Part]) -> bool {
    parts.contains(&Part::StableApi) && !stable_api_due(scope)
}
