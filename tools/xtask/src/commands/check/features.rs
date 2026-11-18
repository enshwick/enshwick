//! The lints of each package with none of its features and with each alone.
//! The workspace build lints each package with the union of the features its
//! members and `host_args` turn on, so code that one feature alone leaves
//! unused, or that builds only beside another feature, passes it and fails
//! the first build that turns on that feature by itself. Only
//! the library and binaries: the dev-dependencies of the other targets turn
//! the features back on and hide the gap, and the workspace's clippy lints
//! those targets. Off Windows the Windows part of them runs them again for
//! the Windows target, where code gated on the target platform differs, for
//! the packages with such code ([`windows_code`]).

use std::fs;
use std::path::Path;

use proc_macro2::{Delimiter, TokenStream, TokenTree};

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
/// Windows-only, or a source file under `root` holds a `cfg` predicate on
/// the target platform ([`gates_target`]).
pub(super) fn windows_code(root: &Path, package: &Package) -> bool {
    if package.windows_only {
        return true;
    }
    let mut files = Vec::new();
    // A false yes, or an unreadable tree, costs only a lint too many.
    rust_files(&root.join(&package.dir).join("src"), &mut files).is_err()
        || files
            .iter()
            .any(|file| fs::read_to_string(file).map_or(true, |text| gates_target(&text)))
}

/// The `cfg` options that tell the Windows target from the host, so code
/// under one builds on only one of them.
const TARGET_OPTIONS: [&str; 5] = [
    "windows",
    "unix",
    "target_os",
    "target_family",
    "target_env",
];

/// Whether `text` holds a `cfg(..)`, `cfg!(..)` or `cfg_attr(..)` whose
/// predicate names one of the [`TARGET_OPTIONS`] (`windows`, `not(unix)`,
/// `target_env = "msvc"`); a comment or a string names nothing, and text
/// that does not lex counts as one that does.
pub(super) fn gates_target(text: &str) -> bool {
    let Ok(tokens) = text.parse::<TokenStream>() else {
        return true;
    };
    let mut streams = vec![tokens];
    while let Some(stream) = streams.pop() {
        let mut after_cfg = false;
        for token in stream {
            match token {
                TokenTree::Ident(ident) => after_cfg = ident == "cfg" || ident == "cfg_attr",
                TokenTree::Punct(punct) if punct.as_char() == '!' => {}
                TokenTree::Group(group) => {
                    if after_cfg
                        && group.delimiter() == Delimiter::Parenthesis
                        && names_target(group.stream())
                    {
                        return true;
                    }
                    streams.push(group.stream());
                    after_cfg = false;
                }
                TokenTree::Punct(_) | TokenTree::Literal(_) => after_cfg = false,
            }
        }
    }
    false
}

/// Whether a `cfg` predicate names one of the [`TARGET_OPTIONS`].
fn names_target(predicate: TokenStream) -> bool {
    predicate.into_iter().any(|token| match token {
        TokenTree::Ident(ident) => TARGET_OPTIONS.iter().any(|option| ident == option),
        TokenTree::Group(group) => names_target(group.stream()),
        TokenTree::Punct(_) | TokenTree::Literal(_) => false,
    })
}
