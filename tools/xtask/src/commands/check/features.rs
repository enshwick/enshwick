//! The lints of each package with none of its features and with each alone.
//! The workspace build lints each package with the union of the features its
//! members and `host_args` turn on, so code that one feature alone leaves
//! unused, or that builds only beside another feature, passes it and fails
//! the first build that turns on that feature by itself. Only
//! the library and binaries: the dev-dependencies of the other targets turn
//! the features back on and hide the gap, and the workspace's clippy lints
//! those targets. Off Windows the Windows part of them runs them again for
//! the Windows target, where code gated on the target platform differs, for
//! the packages with such code ([`windows_code`]). A local check lints each
//! feature alone only where the change can break it ([`Alone`]); CI and the
//! merge queue lint every one.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use proc_macro2::{Delimiter, Group, TokenStream, TokenTree};

use super::Step;
use crate::affected::{self, Package, Scope};
use crate::check::rust_files;

/// Which features [`steps`] lints alone; the run without features is always
/// there.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Alone {
    /// Every feature: CI, the merge queue, a whole-workspace check, or a
    /// change to a shared build file.
    Every,
    /// In a local check, per package whose files changed, the features its
    /// changed source gates, or `None` for every feature; a package left out,
    /// such as a dependent, lints none alone.
    Touched(BTreeMap<String, Option<BTreeSet<String>>>),
}

/// Shared files whose change reaches the features of every package.
const SHARED_BUILD: [&str; 2] = ["Cargo.toml", "Cargo.lock"];
/// A package's files that wire or build its features.
const FEATURE_BUILD: [&str; 2] = ["Cargo.toml", "build.rs"];
/// Package folders of targets the feature lints do not build.
const UNLINTED_DIRS: [&str; 3] = ["tests/", "benches/", "examples/"];
/// A package's source folder, the one the feature lints build.
const SOURCE_DIR: &str = "src/";
/// The modules of a package's crate roots under [`SOURCE_DIR`].
const CRATE_ROOTS: [&str; 2] = ["lib", "main"];

impl Alone {
    /// The features the changed `files` (relative to the root, `/`
    /// separators) of `packages` reach, `texts` giving a file's text at the
    /// merge base and now. The manifest, the build script and Rust outside
    /// `src/`, the test, bench and example targets left out, reach every
    /// feature, as does a source file that is gone or does not lex; a Rust
    /// file under `src/` reaches the features it gates ([`source_gates`]),
    /// a test module none. Any other file (data a source file includes, a
    /// snapshot, a header, docs) reaches none: it moves no `cfg` gate, and
    /// the run without features and the workspace's clippy build what reads
    /// it. A feature that turns a reached one on is reached too.
    pub(super) fn of(files: &[String], packages: &[Package], texts: &dyn affected::Texts) -> Self {
        let mut touched: BTreeMap<String, Option<BTreeSet<String>>> = BTreeMap::new();
        for file in files {
            let Some(package) = affected::owner(file, packages) else {
                if SHARED_BUILD.contains(&file.as_str()) {
                    return Self::Every;
                }
                continue;
            };
            let own = &file[package.dir.len() + 1..];
            let rust = Path::new(own).extension().is_some_and(|ext| ext == "rs");
            let gated = match own.strip_prefix(SOURCE_DIR) {
                _ if FEATURE_BUILD.contains(&own) => None,
                Some(path) if rust && !is_test_module(path) => source_gates(file, path, texts),
                None if rust && !UNLINTED_DIRS.iter().any(|dir| own.starts_with(dir)) => None,
                _ => continue,
            };
            let entry = touched
                .entry(package.name.clone())
                .or_insert_with(|| Some(BTreeSet::new()));
            match (entry.as_mut(), gated) {
                (Some(features), Some(gated)) => features.extend(gated),
                (_, None) => *entry = None,
                (None, Some(_)) => {}
            }
        }
        for package in packages {
            if let Some(Some(features)) = touched.get_mut(&package.name) {
                add_turning_on(package, features);
            }
        }
        Self::Touched(touched)
    }

    /// Whether `feature` of `package` is linted alone.
    fn lints(&self, package: &Package, feature: &str) -> bool {
        match self {
            Self::Every => true,
            Self::Touched(touched) => touched
                .get(&package.name)
                .is_some_and(|gated| gated.as_ref().is_none_or(|gated| gated.contains(feature))),
        }
    }
}

/// The features a change to the source file `file`, at `path` under its
/// package's `src/`, reaches: those its text names at the merge base and
/// now ([`gated_features`]), so a removed gate counts, and those on the
/// `mod` declarations that include it ([`module_gates`]); `None`, every
/// feature, when it is gone or a text does not lex.
fn source_gates(file: &str, path: &str, texts: &dyn affected::Texts) -> Option<BTreeSet<String>> {
    let mut features = gated_features(&texts.after(file)?)?;
    if let Some(before) = texts.before(file) {
        features.extend(gated_features(&before)?);
    }
    let src = &file[..file.len() - path.len()];
    features.extend(module_gates(src, path, |parent| texts.after(parent))?);
    Some(features)
}

/// The features the `cfg` attributes on the `mod` declarations of the
/// module at `path` under `src` and of each module above it name, read
/// from the current parent files (`a/b.rs` is `mod b;` in `a.rs` or
/// `a/mod.rs`, `a.rs` is `mod a;` in `lib.rs` or `main.rs`); `None` when a
/// parent does not lex. The walk stops at a parent it cannot find, such as
/// one a `#[path]` names.
fn module_gates(
    src: &str,
    path: &str,
    read: impl Fn(&str) -> Option<String>,
) -> Option<BTreeSet<String>> {
    let module = path.strip_suffix(".rs").unwrap_or(path);
    let mut names: Vec<&str> = module.split('/').collect();
    // `a/mod.rs` is the module `a`.
    if names.last() == Some(&"mod") {
        names.pop();
    }
    let mut features = BTreeSet::new();
    for depth in (0..names.len()).rev() {
        let (above, name) = (&names[..depth], names[depth]);
        let parents = if above.is_empty() {
            if CRATE_ROOTS.contains(&name) {
                break;
            }
            CRATE_ROOTS.map(|root| format!("{src}{root}.rs"))
        } else {
            let above = above.join("/");
            [format!("{src}{above}.rs"), format!("{src}{above}/mod.rs")]
        };
        let Some(text) = parents.iter().find_map(|parent| read(parent)) else {
            break;
        };
        features.extend(mod_gates(&text, name)?);
    }
    Some(features)
}

/// The features the attributes on `mod name` in `text` name; `None` when
/// the text does not lex.
fn mod_gates(text: &str, name: &str) -> Option<BTreeSet<String>> {
    let mut features = BTreeSet::new();
    let mut streams = vec![text.parse::<TokenStream>().ok()?];
    while let Some(stream) = streams.pop() {
        // The attributes since the last item ended.
        let mut attributes = Vec::new();
        let mut tokens = stream.into_iter().peekable();
        while let Some(token) = tokens.next() {
            match token {
                TokenTree::Group(group) if group.delimiter() == Delimiter::Bracket => {
                    attributes.push(group.stream());
                }
                TokenTree::Group(group) if group.delimiter() == Delimiter::Brace => {
                    streams.push(group.stream());
                    attributes.clear();
                }
                TokenTree::Ident(ident)
                    if ident == "mod"
                        && matches!(tokens.peek(), Some(TokenTree::Ident(next)) if next == name) =>
                {
                    for attribute in attributes.drain(..) {
                        named_features(attribute, &mut features);
                    }
                }
                TokenTree::Punct(punct) if punct.as_char() == ';' => attributes.clear(),
                TokenTree::Group(_)
                | TokenTree::Ident(_)
                | TokenTree::Punct(_)
                | TokenTree::Literal(_) => {}
            }
        }
    }
    Some(features)
}

/// Adds to `features` each feature of `package` whose `[features]` list
/// turns one of them on, directly or through another.
fn add_turning_on(package: &Package, features: &mut BTreeSet<String>) {
    loop {
        let more: Vec<String> = package
            .feature_table
            .iter()
            .filter(|(name, list)| {
                !features.contains(*name) && list.iter().any(|on| features.contains(on))
            })
            .map(|(name, _)| name.clone())
            .collect();
        if more.is_empty() {
            return;
        }
        features.extend(more);
    }
}

/// Whether `path`, under a package's `src/`, is a `tests` module's file or
/// folder, which only test builds compile.
fn is_test_module(path: &str) -> bool {
    Path::new(path)
        .with_extension("")
        .components()
        .any(|part| part.as_os_str() == "tests")
}

/// The packages `scope` checks in full that have features and that `keep`
/// keeps, the ones [`steps`] lints.
pub(super) fn linted<'a>(
    scope: &'a Scope,
    packages: &'a [Package],
    keep: impl Fn(&Package) -> bool + 'a,
) -> impl Iterator<Item = &'a Package> {
    packages
        .iter()
        .filter(move |package| match scope {
            Scope::All => true,
            Scope::Some { code, .. } => code.contains(&package.name),
        })
        .filter(move |package| !package.features.is_empty() && keep(package))
}

/// The runs of `clippy` (its cargo words, such as `["clippy"]` or the
/// cross-compiling `["xwin", "clippy", "--target", ..]`) over the packages
/// [`linted`] gives: one without any feature, then one for each feature
/// `alone` names.
pub(super) fn steps(
    scope: &Scope,
    packages: &[Package],
    keep: impl Fn(&Package) -> bool,
    clippy: &[&str],
    alone: &Alone,
) -> Vec<Step> {
    linted(scope, packages, keep)
        .flat_map(|package| {
            let alone = package
                .features
                .iter()
                .filter(|feature| alone.lints(package, feature))
                .map(Some);
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
/// the target platform ([`gates_target`]) outside test code, which these
/// lints do not build: a `tests` module's file or folder is skipped.
pub(super) fn windows_code(root: &Path, package: &Package) -> bool {
    if package.windows_only {
        return true;
    }
    let src = root.join(&package.dir).join("src");
    let mut files = Vec::new();
    // A false yes, or an unreadable tree, costs only a lint too many.
    rust_files(&src, &mut files).is_err()
        || files
            .iter()
            .filter(|file| {
                file.strip_prefix(&src)
                    .map_or(true, |path| !is_test_module(&path.to_string_lossy()))
            })
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
/// `target_env = "msvc"`); a comment, a string or an item under
/// `#[cfg(test)]` names nothing, and text that does not lex counts as one
/// that does.
pub(super) fn gates_target(text: &str) -> bool {
    cfg_predicates(text).is_none_or(|predicates| predicates.into_iter().any(names_target))
}

/// The features the `cfg(..)`, `cfg!(..)` and `cfg_attr(..)` predicates in
/// `text` name (`feature = "fake"`), as [`gates_target`] reads them; `None`
/// when the text does not lex.
pub(super) fn gated_features(text: &str) -> Option<BTreeSet<String>> {
    let mut features = BTreeSet::new();
    for predicate in cfg_predicates(text)? {
        named_features(predicate, &mut features);
    }
    Some(features)
}

/// Adds the features `predicate` names to `out`.
fn named_features(predicate: TokenStream, out: &mut BTreeSet<String>) {
    let mut tokens = predicate.into_iter().peekable();
    while let Some(token) = tokens.next() {
        match token {
            TokenTree::Ident(ident) if ident == "feature" => {
                if matches!(tokens.peek(), Some(TokenTree::Punct(punct)) if punct.as_char() == '=')
                {
                    tokens.next();
                    if let Some(TokenTree::Literal(name)) = tokens.next() {
                        // A raw string (`r#"fake"#`) names the same feature.
                        let name = name.to_string();
                        let name = name.trim_start_matches('r').trim_matches('#');
                        out.insert(name.trim_matches('"').to_owned());
                    }
                }
            }
            TokenTree::Group(group) => named_features(group.stream(), out),
            TokenTree::Ident(_) | TokenTree::Punct(_) | TokenTree::Literal(_) => {}
        }
    }
}

/// The predicates of the `cfg(..)`, `cfg!(..)` and `cfg_attr(..)` in `text`
/// outside items under `#[cfg(test)]`, comments and strings; `None` when the
/// text does not lex.
fn cfg_predicates(text: &str) -> Option<Vec<TokenStream>> {
    let tokens = text.parse::<TokenStream>().ok()?;
    let mut predicates = Vec::new();
    let mut streams = vec![tokens];
    while let Some(stream) = streams.pop() {
        let mut after_cfg = false;
        let mut in_test_item = false;
        for token in stream {
            if in_test_item {
                // The item ends at its body or its semicolon.
                in_test_item = !matches!(&token, TokenTree::Group(group) if group.delimiter() == Delimiter::Brace)
                    && !matches!(&token, TokenTree::Punct(punct) if punct.as_char() == ';');
                continue;
            }
            match token {
                TokenTree::Ident(ident) => after_cfg = ident == "cfg" || ident == "cfg_attr",
                TokenTree::Punct(punct) if punct.as_char() == '!' => {}
                TokenTree::Group(group) if is_cfg_test(&group) => {
                    in_test_item = true;
                    after_cfg = false;
                }
                TokenTree::Group(group) => {
                    if after_cfg && group.delimiter() == Delimiter::Parenthesis {
                        predicates.push(group.stream());
                    }
                    streams.push(group.stream());
                    after_cfg = false;
                }
                TokenTree::Punct(_) | TokenTree::Literal(_) => after_cfg = false,
            }
        }
    }
    Some(predicates)
}

/// Whether `group` is the `[cfg(test)]` of an attribute.
fn is_cfg_test(group: &Group) -> bool {
    let only = |stream: TokenStream, word: &str| {
        let mut tokens = stream.into_iter();
        matches!(tokens.next(), Some(TokenTree::Ident(ident)) if ident == word)
            && tokens.next().is_none()
    };
    let mut tokens = group.stream().into_iter();
    group.delimiter() == Delimiter::Bracket
        && matches!(tokens.next(), Some(TokenTree::Ident(cfg)) if cfg == "cfg")
        && matches!(tokens.next(), Some(TokenTree::Group(predicate))
            if predicate.delimiter() == Delimiter::Parenthesis && only(predicate.stream(), "test"))
        && tokens.next().is_none()
}

/// Whether a `cfg` predicate names one of the [`TARGET_OPTIONS`].
fn names_target(predicate: TokenStream) -> bool {
    predicate.into_iter().any(|token| match token {
        TokenTree::Ident(ident) => TARGET_OPTIONS.iter().any(|option| ident == option),
        TokenTree::Group(group) => names_target(group.stream()),
        TokenTree::Punct(_) | TokenTree::Literal(_) => false,
    })
}
