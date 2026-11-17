//! The lints of each package with none of its features and with each alone.
//! The workspace build lints each package with the union of the features its
//! members and `host_args` turn on, so code that one feature alone leaves
//! unused, or that builds only beside another feature, passes it and fails
//! the first build that turns on that feature by itself. Only
//! the library and binaries: the dev-dependencies of the other targets turn
//! the features back on and hide the gap, and the workspace's clippy lints
//! those targets. Off Windows the Windows lint runs them again for the
//! Windows target, where the `cfg(windows)` code builds, for the packages
//! with such code ([`windows_code`]).

use std::fs;
use std::path::Path;

use super::Step;
use crate::affected::{Package, Scope};
use crate::check::rust_files;

/// The runs of `clippy` (its cargo words, such as `["clippy"]` or the
/// cross-compiling `["xwin", "clippy", "--target", ..]`) over the packages
/// `scope` checks in full that have features and that `keep` keeps: one
/// without any feature, then one for each feature alone.
pub(super) fn steps(
    scope: &Scope,
    packages: &[Package],
    keep: impl Fn(&Package) -> bool,
    clippy: &[&str],
) -> Vec<Step> {
    packages
        .iter()
        .filter(|package| match scope {
            Scope::All => true,
            Scope::Some { code, .. } => code.contains(&package.name),
        })
        .filter(|package| !package.features.is_empty() && keep(package))
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

/// Whether `package` has code that builds only for Windows, so its lints
/// for the Windows target can differ from those on the host: it is
/// Windows-only, or a source file under `root` names `windows` (a
/// `cfg(windows)`, a `target_os = "windows"`, a comment).
pub(super) fn windows_code(root: &Path, package: &Package) -> bool {
    if package.windows_only {
        return true;
    }
    let mut files = Vec::new();
    // A false yes, or an unreadable tree, costs only a lint too many.
    rust_files(&root.join(&package.dir).join("src"), &mut files).is_err()
        || files
            .iter()
            .any(|file| fs::read_to_string(file).map_or(true, |text| names_windows(&text)))
}

/// Whether `text` holds `windows` as a word of its own, not inside an
/// identifier such as `windows_sys`.
pub(super) fn names_windows(text: &str) -> bool {
    let ident = |c: char| c.is_alphanumeric() || c == '_';
    text.match_indices("windows").any(|(at, word)| {
        !text[..at].ends_with(ident) && !text[at + word.len()..].starts_with(ident)
    })
}
