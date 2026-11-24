//! Which packages a change touches: the packages whose files changed and
//! every workspace package that depends on them. A local check leaves the
//! widest part to CI and the merge queue ([`Lane`]).

pub(crate) mod idents;
mod shared;
mod versions;

use std::collections::{BTreeMap, BTreeSet};

/// A workspace package, as far as choosing checks needs it.
#[derive(Debug, Clone, Default)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "each flag is a separate fact of the package's manifest, not a state"
)]
pub(crate) struct Package {
    pub(crate) name: String,
    /// The package's version.
    pub(crate) version: String,
    /// Directory relative to the repository root, with `/` separators.
    pub(crate) dir: String,
    /// Workspace packages it depends on, in any dependency kind, those of
    /// [`Self::doc_deps`] left out.
    pub(crate) deps: Vec<String>,
    /// Workspace dev-dependencies only its doc examples can use: its
    /// library, its only crate with tests, gives itself their name
    /// ([`idents::self_alias`]), so its own code reaches itself by that
    /// name, and a change to one reaches its examples, not its tests. The
    /// API's domain crates dev-depend on the facade for their examples,
    /// which made a change to any of them reach all 26 packages
    /// (2026-11-16).
    pub(crate) doc_deps: Vec<String>,
    /// The name of its library target, as Rust paths name it, if it has one.
    pub(crate) lib: Option<String>,
    /// Builds only for Windows (`package.metadata.xtask.windows-only`):
    /// off Windows it stays out of host builds.
    pub(crate) windows_only: bool,
    /// Has tests of its Windows code, run under Wine off Windows
    /// (`package.metadata.xtask.windows-tests`, or `windows-only`).
    pub(crate) windows_tests: bool,
    /// Compiles C for its target, itself or through a dependency
    /// (`package.metadata.xtask.compiles-c`): a check for another target
    /// than the host's would need a cross compiler for it.
    pub(crate) compiles_c: bool,
    /// Has a stable surface to check in a build without `unstable`: an
    /// `unstable` feature of its own, or examples that name the facade as a
    /// stable mod does (a dev-dependency on it with no feature beyond `test`).
    pub(crate) stable_build: bool,
    /// Its own features, `default` left out, which the host check lints one
    /// at a time.
    pub(crate) features: Vec<String>,
    /// What each of its features turns on, as its `[features]` table lists
    /// it (`unstable`, `dep:x`, `x/typed`).
    pub(crate) feature_table: BTreeMap<String, Vec<String>>,
}

/// What a change needs checked.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Scope {
    /// A file every package builds with changed: check the whole workspace.
    All,
    /// Only these packages.
    Some {
        /// Packages checked in full (lints, tests, examples, and for the
        /// facade its stable surface and, unless `wide`, its trybuild test
        /// and stable examples): the
        /// changed ones and their dependents, or when `wide` the changed
        /// ones alone.
        code: BTreeSet<String>,
        /// Packages whose doctests changed without their code: the book's
        /// chapters are doctests of the API crate, and a change that is not
        /// wide reaches the examples of the packages whose examples alone
        /// use a changed one ([`Package::doc_deps`]).
        docs: BTreeSet<String>,
        /// Packages a local check runs the tests of but not the examples,
        /// leaving those, the facade's trybuild test and its stable examples
        /// to CI: the dependents of a change that reaches
        /// [`WIDE_PACKAGES`] or more.
        tested: BTreeSet<String>,
        /// Packages a local check only builds and lints, leaving their tests
        /// to CI: those whose versions alone moved.
        built: BTreeSet<String>,
        /// A local check of a change that reaches [`WIDE_PACKAGES`] or
        /// more, changed ones included: it leaves the
        /// facade's trybuild test and stable examples and the dependents'
        /// rustdoc to CI, and runs the dependents' tests one process per
        /// binary.
        wide: bool,
    },
}

/// Where a check runs. CI and the merge queue check every package a change
/// reaches in full; a local check, which must stay under ten minutes, runs
/// only the tests of the dependents of a change that reaches
/// [`WIDE_PACKAGES`] or more, and only builds a package whose versions
/// alone moved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Lane {
    Local,
    Ci,
}

impl Lane {
    /// CI when the `CI` variable is set, as every CI service sets it.
    pub(crate) fn current() -> Self {
        Self::from_ci(std::env::var("CI").ok().as_deref())
    }

    /// The lane a value of `CI` names: an empty, `false` or `0` value turns
    /// CI mode off, as tools that read the variable take it.
    fn from_ci(value: Option<&str>) -> Self {
        match value.map(str::trim) {
            None | Some("" | "0") => Self::Local,
            Some(value) if value.eq_ignore_ascii_case("false") => Self::Local,
            Some(_) => Self::Ci,
        }
    }
}

/// The fewest packages, changed ones and their dependents together, that
/// make a local check wide: one changed package and eight that depend on
/// it. From it on the dependents' examples and rustdoc, the facade's
/// trybuild test and its stable examples go to CI. A change to any API crate reaches 23 to
/// 27 packages (2026-11-17), whose examples, trybuild test and stable
/// surface checked in full went past the ten-minute local limit on the
/// Windows PC (669 s, 2026-11-14).
pub(crate) const WIDE_PACKAGES: usize = 9;

/// A package's own changelog and change files, which no build reads.
const CHANGELOG: &str = "CHANGELOG.md";
const CHANGES_DIR: &str = "changes/";
/// A package's readme: documentation, which no build reads unless its
/// crate's docs include it, so its examples are the most it can break; no
/// crate includes one today, and its doctests run so one that does stays
/// checked.
const README: &str = "README.md";
/// A package's manifest, relative to its directory.
const PACKAGE_MANIFEST: &str = "Cargo.toml";

/// The text of a file before and after the change, for the shared build
/// files whose content says which packages they reach.
pub(crate) trait Texts {
    /// At the merge base; `None` when it was not there.
    fn before(&self, path: &str) -> Option<String>;
    /// In the working tree; `None` when it is gone.
    fn after(&self, path: &str) -> Option<String>;
}

/// Files whose every change reaches every package; Cargo still reads its
/// settings under the legacy name `.cargo/config`.
const GLOBAL_FILES: &[&str] = &[
    "rust-toolchain.toml",
    "clippy.toml",
    ".cargo/config.toml",
    ".cargo/config",
];
/// Directories of tool settings every test run reads, such as nextest's
/// `.config/nextest.toml`.
const GLOBAL_DIRS: &[&str] = &[".config/"];
/// The book's chapters, compiled as doctests of the API crate.
const BOOK_DIR: &str = "book/src/";
const BOOK_HARNESS: &str = "enshwick";
/// The package whose tests compile and type-check the book's Luau examples.
const BOOK_LUAU: &str = "enshwick-luau";

/// Sorts changed files (relative to the root, `/` separators) into a scope
/// for `lane`; `texts` gives the shared build files and the manifests before
/// and after.
pub(crate) fn scope(
    files: &[String],
    packages: &[Package],
    texts: &dyn Texts,
    lane: Lane,
) -> Scope {
    let local = lane == Lane::Local;
    let mut changed = BTreeSet::new();
    let mut bumped = BTreeSet::new();
    let mut docs = BTreeSet::new();
    let mut luau = false;
    for file in files {
        match file.as_str() {
            shared::LOCK if local && bump(texts, file, versions::lock) => {}
            shared::MANIFEST if local && bump(texts, file, versions::root) => {
                // Members that inherit the version build under the new one.
                bumped.extend(shared::inherited(texts, packages, local).unwrap_or_default());
            }
            shared::LOCK => {
                let Some(reached) = shared::locked(texts, packages) else {
                    return Scope::All;
                };
                changed.extend(reached);
            }
            shared::MANIFEST => {
                let Some(reached) = shared::inherited(texts, packages, local) else {
                    return Scope::All;
                };
                changed.extend(reached);
            }
            path if GLOBAL_FILES.contains(&path)
                || GLOBAL_DIRS.iter().any(|dir| path.starts_with(dir)) =>
            {
                return Scope::All;
            }
            path if path.starts_with(BOOK_DIR) => {
                docs.insert(BOOK_HARNESS.to_owned());
                luau = true;
            }
            path => {
                let Some(package) = owner(path, packages) else {
                    continue;
                };
                let own = &path[package.dir.len() + 1..];
                if own == CHANGELOG || own.starts_with(CHANGES_DIR) {
                    continue;
                }
                if own == README {
                    docs.insert(package.name.clone());
                    continue;
                }
                if local && own == PACKAGE_MANIFEST && bump(texts, path, versions::member) {
                    bumped.insert(package.name.clone());
                } else {
                    changed.insert(package.name.clone());
                }
            }
        }
    }
    let mut code = dependents(&changed, packages);
    // Its own tests read the chapters and run luau-lsp; what depends on it does not.
    if luau && packages.iter().any(|package| package.name == BOOK_LUAU) {
        code.insert(BOOK_LUAU.to_owned());
        changed.insert(BOOK_LUAU.to_owned());
    }
    let wide = local && code.len() >= WIDE_PACKAGES;
    // A wide change leaves the examples of what it reaches to CI.
    if !wide {
        docs.extend(examples_of(&code, packages));
    }
    let mut tested = BTreeSet::new();
    if wide {
        tested = code.difference(&changed).cloned().collect();
        code = changed;
    }
    let mut built = bumped;
    built.retain(|name| !code.contains(name) && !tested.contains(name));
    docs.retain(|name| !code.contains(name));
    Scope::Some {
        code,
        docs,
        tested,
        built,
        wide,
    }
}

/// Whether the file at `path` changed only in versions, as `only_versions`
/// judges its text before and after.
fn bump(texts: &dyn Texts, path: &str, only_versions: fn(&str, &str) -> bool) -> bool {
    texts
        .before(path)
        .zip(texts.after(path))
        .is_some_and(|(before, after)| only_versions(&before, &after))
}

/// The package whose directory holds `file`, the innermost one.
pub(crate) fn owner<'a>(file: &str, packages: &'a [Package]) -> Option<&'a Package> {
    packages
        .iter()
        .filter(|package| file.starts_with(&format!("{}/", package.dir)))
        .max_by_key(|package| package.dir.len())
}

/// `changed` and every package that depends on one of them, directly or not.
pub(crate) fn dependents(changed: &BTreeSet<String>, packages: &[Package]) -> BTreeSet<String> {
    let mut found = changed.clone();
    loop {
        let before = found.len();
        for package in packages {
            if package.deps.iter().any(|dep| found.contains(dep)) {
                found.insert(package.name.clone());
            }
        }
        if found.len() == before {
            return found;
        }
    }
}

/// The packages outside `code` whose doc examples use one of `code`
/// ([`Package::doc_deps`]).
fn examples_of(code: &BTreeSet<String>, packages: &[Package]) -> BTreeSet<String> {
    packages
        .iter()
        .filter(|package| !code.contains(&package.name))
        .filter(|package| package.doc_deps.iter().any(|dep| code.contains(dep)))
        .map(|package| package.name.clone())
        .collect()
}

#[cfg(test)]
mod tests;
