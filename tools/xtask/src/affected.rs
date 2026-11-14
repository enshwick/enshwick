//! Which packages a change touches: the packages whose files changed and
//! every workspace package that depends on them. A local check leaves the
//! widest part to CI and the merge queue ([`Lane`]).

mod shared;
mod versions;

use std::collections::BTreeSet;

/// A workspace package, as far as choosing checks needs it.
#[derive(Debug, Clone)]
pub(crate) struct Package {
    pub(crate) name: String,
    /// The package's version.
    pub(crate) version: String,
    /// Directory relative to the repository root, with `/` separators.
    pub(crate) dir: String,
    /// Workspace packages it depends on, in any dependency kind.
    pub(crate) deps: Vec<String>,
    /// The name of its library target, as Rust paths name it, if it has one.
    pub(crate) lib: Option<String>,
    /// Builds only for Windows (`package.metadata.xtask.windows-only`):
    /// off Windows it stays out of host builds.
    pub(crate) windows_only: bool,
    /// Has tests of its Windows code, run under Wine off Windows
    /// (`package.metadata.xtask.windows-tests`, or `windows-only`).
    pub(crate) windows_tests: bool,
    /// Has a stable surface to check in a build without `unstable`: an
    /// `unstable` feature of its own, or examples that name the facade as a
    /// stable mod does (a dev-dependency on it with no feature beyond `test`).
    pub(crate) stable_build: bool,
}

/// What a change needs checked.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Scope {
    /// A file every package builds with changed: check the whole workspace.
    All,
    /// Only these packages.
    Some {
        /// Packages checked in full (lints, tests, examples, and for the
        /// facade its trybuild test and stable surface): the changed ones
        /// and their dependents, or locally past [`MAX_LOCAL_DEPENDENTS`]
        /// the changed ones alone.
        code: BTreeSet<String>,
        /// Packages whose doctests changed without their code: the book's
        /// chapters are doctests of the API crate.
        docs: BTreeSet<String>,
        /// Packages a local check runs the tests of but not the examples,
        /// leaving those, the facade's trybuild test and its stable surface
        /// to CI: the dependents of a change more packages depend on than
        /// [`MAX_LOCAL_DEPENDENTS`].
        tested: BTreeSet<String>,
        /// Packages a local check only builds and lints, leaving their tests
        /// to CI: those whose versions alone moved.
        built: BTreeSet<String>,
    },
}

/// Where a check runs. CI and the merge queue check every package a change
/// reaches in full; a local check, which must stay under ten minutes, runs
/// only the tests of the dependents of a change many packages depend on,
/// and only builds a package whose versions alone moved.
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

/// The most dependents a local check checks in full beside the changed
/// packages; past it only their tests run here. Through the dev-dependencies
/// on the facade, a change to any API crate reaches about 24 packages, whose
/// examples, trybuild test and stable surface checked in full go past the
/// ten-minute local limit on the Windows PC (2026-11-14).
pub(crate) const MAX_LOCAL_DEPENDENTS: usize = 8;

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
                bumped.extend(shared::inherited(texts, packages).unwrap_or_default());
            }
            shared::LOCK => {
                let Some(reached) = shared::locked(texts, packages) else {
                    return Scope::All;
                };
                changed.extend(reached);
            }
            shared::MANIFEST => {
                let Some(reached) = shared::inherited(texts, packages) else {
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
    let mut tested = BTreeSet::new();
    if local && code.len() - changed.len() > MAX_LOCAL_DEPENDENTS {
        tested = code.difference(&changed).cloned().collect();
        code = changed;
    }
    // Its own tests read the chapters and run luau-lsp; what depends on it does not.
    if luau && packages.iter().any(|package| package.name == BOOK_LUAU) {
        code.insert(BOOK_LUAU.to_owned());
    }
    tested.retain(|name| !code.contains(name));
    let mut built = bumped;
    built.retain(|name| !code.contains(name) && !tested.contains(name));
    docs.retain(|name| !code.contains(name));
    Scope::Some {
        code,
        docs,
        tested,
        built,
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
fn owner<'a>(file: &str, packages: &'a [Package]) -> Option<&'a Package> {
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    fn package(name: &str, dir: &str, deps: &[&str]) -> Package {
        Package {
            name: name.to_owned(),
            version: "0.0.0-dev".to_owned(),
            dir: dir.to_owned(),
            deps: deps.iter().map(|d| (*d).to_owned()).collect(),
            lib: None,
            windows_only: false,
            windows_tests: false,
            stable_build: false,
        }
    }

    fn workspace() -> Vec<Package> {
        vec![
            package("macros", "crates/internal/macros", &[]),
            package("enshwick", "crates/enshwick", &["macros"]),
            package("loader", "crates/runtime/loader", &["enshwick"]),
            package("trading-post", "mods/trading-post", &["enshwick"]),
            package("haggling", "mods/haggling", &["trading-post"]),
            package("my-mod", "mods/my-mod", &["enshwick"]),
        ]
    }

    fn files(list: &[&str]) -> Vec<String> {
        list.iter().map(|f| (*f).to_owned()).collect()
    }

    fn names(list: &[&str]) -> BTreeSet<String> {
        list.iter().map(|n| (*n).to_owned()).collect()
    }

    /// The shared files before and after a change, and every member's
    /// manifest as it is after.
    #[derive(Default)]
    struct Tree {
        before: BTreeMap<String, String>,
        after: BTreeMap<String, String>,
    }

    impl Tree {
        fn changed(path: &str, before: &str, after: &str) -> Self {
            let mut tree = Self::default();
            for package in workspace() {
                tree.after.insert(
                    format!("{}/Cargo.toml", package.dir),
                    format!("[package]\nname = \"{}\"\n", package.name),
                );
            }
            tree.before.insert(path.to_owned(), before.to_owned());
            tree.after.insert(path.to_owned(), after.to_owned());
            tree
        }

        fn member(mut self, dir: &str, manifest: &str) -> Self {
            self.after
                .insert(format!("{dir}/Cargo.toml"), manifest.to_owned());
            self
        }
    }

    impl Texts for Tree {
        fn before(&self, path: &str) -> Option<String> {
            self.before.get(path).cloned()
        }

        fn after(&self, path: &str) -> Option<String> {
            self.after.get(path).cloned()
        }
    }

    /// The scope of a change to files whose content the scope does not read.
    fn scope(files: &[String], packages: &[Package]) -> Scope {
        super::scope(files, packages, &Tree::default(), Lane::Ci)
    }

    fn code(scope: Scope) -> BTreeSet<String> {
        let Scope::Some { code, .. } = scope else {
            panic!("the whole workspace");
        };
        code
    }

    /// The workspace's lock file: `macros` builds on `syn`, the loader on
    /// `windows` 0.1 and `my-mod` on `windows` 0.2.
    fn lock(windows_checksum: &str) -> String {
        let registry = "registry+https://github.com/rust-lang/crates.io-index";
        format!(
            r#"version = 4

[[package]]
name = "enshwick"
version = "0.0.0-dev"
dependencies = ["macros"]

[[package]]
name = "haggling"
version = "0.0.0-dev"
dependencies = ["trading-post"]

[[package]]
name = "loader"
version = "0.0.0-dev"
dependencies = ["enshwick", "windows 0.1.0"]

[[package]]
name = "macros"
version = "0.0.0-dev"
dependencies = ["syn"]

[[package]]
name = "my-mod"
version = "0.0.0-dev"
dependencies = ["enshwick", "windows 0.2.0"]

[[package]]
name = "syn"
version = "2.0.0"
source = "{registry}"
checksum = "aa"

[[package]]
name = "trading-post"
version = "0.0.0-dev"
dependencies = ["enshwick"]

[[package]]
name = "windows"
version = "0.1.0"
source = "{registry}"
checksum = "{windows_checksum}"

[[package]]
name = "windows"
version = "0.2.0"
source = "{registry}"
checksum = "cc"
"#
        )
    }

    fn manifest(workspace: &str) -> String {
        format!("[workspace]\nmembers = [\"crates/*\", \"mods/*\"]\n{workspace}")
    }

    #[test]
    fn a_toolchain_lint_cargo_or_nextest_config_change_checks_everything() {
        for file in [
            "rust-toolchain.toml",
            "clippy.toml",
            ".cargo/config.toml",
            ".cargo/config",
            ".config/nextest.toml",
        ] {
            assert_eq!(scope(&files(&[file]), &workspace()), Scope::All, "{file}");
        }
    }

    #[test]
    fn a_lock_change_checks_only_the_packages_built_on_the_changed_entry() {
        let tree = Tree::changed("Cargo.lock", &lock("bb"), &lock("b2"));
        assert_eq!(
            code(super::scope(
                &files(&["Cargo.lock"]),
                &workspace(),
                &tree,
                Lane::Ci
            )),
            names(&["loader"])
        );
    }

    #[test]
    fn a_lock_change_reaches_the_dependents_of_a_package_built_on_it() {
        let tree = Tree::changed(
            "Cargo.lock",
            &lock("bb"),
            &lock("bb").replace("checksum = \"aa\"", "checksum = \"a2\""),
        );
        assert_eq!(
            code(super::scope(
                &files(&["Cargo.lock"]),
                &workspace(),
                &tree,
                Lane::Ci
            )),
            names(&[
                "macros",
                "enshwick",
                "loader",
                "trading-post",
                "haggling",
                "my-mod"
            ])
        );
    }

    #[test]
    fn a_lock_that_does_not_read_checks_everything() {
        let tree = Tree::changed("Cargo.lock", &lock("bb"), "[[package]");
        assert_eq!(
            super::scope(&files(&["Cargo.lock"]), &workspace(), &tree, Lane::Ci),
            Scope::All
        );
    }

    #[test]
    fn a_workspace_dependency_change_checks_the_members_that_inherit_it() {
        let tree = Tree::changed(
            "Cargo.toml",
            &manifest("[workspace.dependencies]\nserde = \"1.0.1\"\nsyn = \"2\"\n"),
            &manifest("[workspace.dependencies]\nserde = \"1.0.2\"\nsyn = \"2\"\n"),
        )
        .member(
            "mods/trading-post",
            "[dev-dependencies]\nserde.workspace = true\nsyn.workspace = true\n",
        )
        .member(
            "crates/runtime/loader",
            "[target.'cfg(windows)'.dependencies]\nserde = { workspace = true }\n",
        )
        .member(
            "crates/internal/macros",
            "[dependencies]\nsyn.workspace = true\n",
        );
        assert_eq!(
            code(super::scope(
                &files(&["Cargo.toml"]),
                &workspace(),
                &tree,
                Lane::Ci
            )),
            names(&["trading-post", "haggling", "loader"])
        );
    }

    #[test]
    fn a_workspace_package_field_change_checks_the_members_that_inherit_it() {
        let tree = Tree::changed(
            "Cargo.toml",
            &manifest("[workspace.package]\nrust-version = \"1.99\"\nedition = \"2024\"\n"),
            &manifest("[workspace.package]\nrust-version = \"1.100\"\nedition = \"2024\"\n"),
        )
        .member("mods/my-mod", "[package]\nrust-version.workspace = true\n")
        .member("mods/haggling", "[package]\nedition.workspace = true\n");
        assert_eq!(
            code(super::scope(
                &files(&["Cargo.toml"]),
                &workspace(),
                &tree,
                Lane::Ci
            )),
            names(&["my-mod"])
        );
    }

    #[test]
    fn a_member_whose_manifest_does_not_read_is_checked() {
        let tree = Tree::changed(
            "Cargo.toml",
            &manifest("[workspace.dependencies]\nserde = \"1.0.1\"\n"),
            &manifest("[workspace.dependencies]\nserde = \"1.0.2\"\n"),
        )
        .member("mods/my-mod", "[dependencies");
        assert_eq!(
            code(super::scope(
                &files(&["Cargo.toml"]),
                &workspace(),
                &tree,
                Lane::Ci
            )),
            names(&["my-mod"])
        );
    }

    #[test]
    fn a_lint_profile_or_patch_change_in_the_root_manifest_checks_everything() {
        for (before, after) in [
            (
                "[workspace.lints.rust]\nunsafe_code = \"warn\"\n",
                "[workspace.lints.rust]\nunsafe_code = \"deny\"\n",
            ),
            (
                "[profile.release]\nlto = true\n",
                "[profile.release]\nlto = false\n",
            ),
            ("", "[patch.crates-io]\nsyn = { path = \"../syn\" }\n"),
        ] {
            let tree = Tree::changed("Cargo.toml", &manifest(before), &manifest(after));
            assert_eq!(
                super::scope(&files(&["Cargo.toml"]), &workspace(), &tree, Lane::Ci),
                Scope::All,
                "{after}"
            );
        }
    }

    #[test]
    fn a_membership_change_checks_everything() {
        // A changed glob or exclude can make a path dependency a member,
        // whose own files and lock entry stay the same.
        for (before, after) in [
            ("", "exclude = [\"crates/runtime/legacy\"]\n"),
            ("", "members = [\"vendor/*\"]\n"),
        ] {
            let tree = Tree::changed(
                "Cargo.toml",
                &format!("[workspace]\n{before}"),
                &format!("[workspace]\n{after}"),
            );
            assert_eq!(
                super::scope(&files(&["Cargo.toml"]), &workspace(), &tree, Lane::Ci),
                Scope::All,
                "{after}"
            );
        }
    }

    #[test]
    fn a_workspace_dependency_change_reaches_every_package_built_on_it() {
        // Features unify: a member that gets `windows` without inheriting it
        // builds with the new features too.
        let mut tree = Tree::changed(
            "Cargo.toml",
            &manifest(
                "[workspace.dependencies]\nwin = { package = \"windows\", version = \"0.1\" }\n",
            ),
            &manifest(
                "[workspace.dependencies]\nwin = { package = \"windows\", version = \"0.1\", features = [\"Win32\"] }\n",
            ),
        );
        tree.after.insert("Cargo.lock".to_owned(), lock("bb"));
        assert_eq!(
            code(super::scope(
                &files(&["Cargo.toml"]),
                &workspace(),
                &tree,
                Lane::Ci
            )),
            names(&["loader", "my-mod"])
        );
    }

    #[test]
    fn a_mod_change_checks_the_mod_and_its_dependents_only() {
        let scope = scope(&files(&["mods/trading-post/src/lib.rs"]), &workspace());
        assert_eq!(
            scope,
            Scope::Some {
                code: names(&["trading-post", "haggling"]),
                docs: names(&[]),
                tested: names(&[]),
                built: names(&[]),
            }
        );
    }

    #[test]
    fn an_api_change_reaches_every_package_built_on_it() {
        let Scope::Some { code, .. } =
            scope(&files(&["crates/enshwick/src/load.rs"]), &workspace())
        else {
            panic!("not all");
        };
        assert_eq!(
            code,
            names(&["enshwick", "loader", "trading-post", "haggling", "my-mod"])
        );
    }

    #[test]
    fn a_book_chapter_runs_only_the_api_doctests() {
        assert_eq!(
            scope(&files(&["book/src/hooks.md"]), &workspace()),
            Scope::Some {
                code: names(&[]),
                docs: names(&["enshwick"]),
                tested: names(&[]),
                built: names(&[]),
            }
        );
    }

    #[test]
    fn a_book_chapter_also_tests_the_luau_runtime_that_checks_its_examples() {
        let mut packages = workspace();
        packages.push(package(
            "enshwick-luau",
            "crates/runtime/luau",
            &["enshwick"],
        ));
        packages.push(package(
            "loader-on-luau",
            "crates/runtime/loader2",
            &["enshwick-luau"],
        ));
        assert_eq!(
            scope(&files(&["book/src/luau.md"]), &packages),
            Scope::Some {
                code: names(&["enshwick-luau"]),
                docs: names(&["enshwick"]),
                tested: names(&[]),
                built: names(&[]),
            }
        );
    }

    #[test]
    fn a_new_luau_lsp_pin_tests_the_luau_runtime_that_runs_it() {
        let mut packages = workspace();
        packages.push(package(
            "cargo-enshwick",
            "crates/cargo-enshwick",
            &["enshwick"],
        ));
        packages.push(package(
            "enshwick-luau",
            "crates/runtime/luau",
            &["enshwick", "cargo-enshwick"],
        ));
        assert_eq!(
            scope(&files(&["crates/cargo-enshwick/luau-lsp.toml"]), &packages),
            Scope::Some {
                code: names(&["cargo-enshwick", "enshwick-luau"]),
                docs: names(&[]),
                tested: names(&[]),
                built: names(&[]),
            }
        );
    }

    #[test]
    fn files_outside_any_package_need_no_cargo_check() {
        assert_eq!(
            scope(
                &files(&["README.md", ".github/workflows/ci.yml", "lefthook.yml"]),
                &workspace()
            ),
            Scope::Some {
                code: names(&[]),
                docs: names(&[]),
                tested: names(&[]),
                built: names(&[]),
            }
        );
    }

    #[test]
    fn a_file_belongs_to_the_innermost_package_and_not_to_a_name_prefix() {
        let mut packages = workspace();
        packages.push(package("nested", "mods/trading-post/nested", &[]));
        let Scope::Some { code, .. } = scope(
            &files(&["mods/trading-post/nested/src/lib.rs", "mods/haggling/x.rs"]),
            &packages,
        ) else {
            panic!("not all");
        };
        assert_eq!(code, names(&["nested", "haggling"]));
    }

    #[test]
    fn a_package_s_changelog_and_change_files_need_no_check_in_any_lane() {
        let files = files(&[
            "mods/trading-post/CHANGELOG.md",
            "mods/trading-post/changes/new-stall.md",
        ]);
        for lane in [Lane::Ci, Lane::Local] {
            assert_eq!(
                super::scope(&files, &workspace(), &Tree::default(), lane),
                Scope::Some {
                    code: names(&[]),
                    docs: names(&[]),
                    tested: names(&[]),
                    built: names(&[]),
                },
                "{lane:?}"
            );
        }
    }

    #[test]
    fn a_package_s_readme_runs_its_examples_and_tests_no_dependent() {
        assert_eq!(
            scope(&files(&["crates/enshwick/README.md"]), &workspace()),
            Scope::Some {
                code: names(&[]),
                docs: names(&["enshwick"]),
                tested: names(&[]),
                built: names(&[]),
            }
        );
    }

    /// A release's change: every member's version, the root's path entries
    /// and the lock's member versions move from `alpha.14` to `alpha.15`.
    fn release() -> (Vec<String>, Tree) {
        let mut tree = Tree::default();
        let mut changed = vec!["Cargo.toml".to_owned(), "Cargo.lock".to_owned()];
        for package in workspace() {
            let path = format!("{}/Cargo.toml", package.dir);
            let manifest = format!(
                "[package]\nname = \"{}\"\nversion = \"1.0.0-alpha.14\"\n[dependencies]\nserde.workspace = true\n",
                package.name
            );
            tree.after
                .insert(path.clone(), manifest.replace("alpha.14", "alpha.15"));
            tree.before.insert(path.clone(), manifest);
            changed.push(path);
        }
        let root = manifest(
            "[workspace.dependencies]\nenshwick = { path = \"crates/enshwick\", version = \"=1.0.0-alpha.14\" }\nserde = \"1\"\n",
        );
        let lock = lock("bb").replace("0.0.0-dev", "1.0.0-alpha.14");
        for (path, text) in [("Cargo.toml", root), ("Cargo.lock", lock)] {
            tree.after
                .insert(path.to_owned(), text.replace("alpha.14", "alpha.15"));
            tree.before.insert(path.to_owned(), text);
        }
        (changed, tree)
    }

    #[test]
    fn a_release_s_versions_alone_are_only_built_locally() {
        let (files, tree) = release();
        let everyone = names(&[
            "macros",
            "enshwick",
            "loader",
            "trading-post",
            "haggling",
            "my-mod",
        ]);
        assert_eq!(
            super::scope(&files, &workspace(), &tree, Lane::Local),
            Scope::Some {
                code: names(&[]),
                docs: names(&[]),
                tested: names(&[]),
                built: everyone.clone(),
            }
        );
        assert_eq!(
            code(super::scope(&files, &workspace(), &tree, Lane::Ci)),
            everyone
        );
    }

    #[test]
    fn members_inheriting_the_workspace_version_are_built_on_its_bump() {
        let root = manifest("[workspace.package]\nversion = \"1.0.0-alpha.14\"\n");
        let tree = Tree::changed("Cargo.toml", &root, &root.replace("alpha.14", "alpha.15"))
            .member("mods/my-mod", "[package]\nversion.workspace = true\n");
        assert_eq!(
            super::scope(&files(&["Cargo.toml"]), &workspace(), &tree, Lane::Local),
            Scope::Some {
                code: names(&[]),
                docs: names(&[]),
                tested: names(&[]),
                built: names(&["my-mod"]),
            }
        );
    }

    #[test]
    fn only_a_ci_value_that_turns_ci_on_picks_the_ci_lane() {
        for value in [None, Some(""), Some("0"), Some("false"), Some("False")] {
            assert_eq!(Lane::from_ci(value), Lane::Local, "{value:?}");
        }
        for value in ["true", "1", "yes"] {
            assert_eq!(Lane::from_ci(Some(value)), Lane::Ci, "{value}");
        }
    }

    #[test]
    fn a_version_beside_another_change_in_a_manifest_is_tested() {
        let (files, mut tree) = release();
        let path = "mods/my-mod/Cargo.toml";
        let after = tree.after[path].replace("serde.workspace", "toml.workspace");
        tree.after.insert(path.to_owned(), after);
        let Scope::Some { code, built, .. } =
            super::scope(&files, &workspace(), &tree, Lane::Local)
        else {
            panic!("the whole workspace");
        };
        assert_eq!(code, names(&["my-mod"]));
        assert!(!built.contains("my-mod"));
    }

    /// The API's shape: the facade over the domain crates over the base
    /// crates, whose examples, like the tools', name the facade through a
    /// dev-dependency, so a change to any of them reaches nearly everything.
    fn api_workspace() -> Vec<Package> {
        vec![
            package("values", "crates/internal/values", &["enshwick"]),
            package("abi", "crates/internal/abi", &["values", "enshwick"]),
            package("core", "crates/internal/core", &["values", "abi"]),
            package("world", "crates/internal/world", &["core", "enshwick"]),
            package("ui", "crates/internal/ui", &["core", "enshwick"]),
            package(
                "enshwick",
                "crates/enshwick",
                &["values", "abi", "core", "world", "ui"],
            ),
            package("loader", "crates/runtime/loader", &["enshwick"]),
            package("luau", "crates/runtime/luau", &["core", "enshwick"]),
            package(
                "cargo-enshwick",
                "crates/cargo-enshwick",
                &["abi", "core", "enshwick"],
            ),
            package("horn", "examples/horn", &["enshwick"]),
            package("waypoints", "examples/waypoints", &["enshwick"]),
        ]
    }

    #[test]
    fn locally_a_change_with_many_dependents_runs_only_their_tests() {
        let packages = api_workspace();
        let files = files(&["crates/internal/world/src/daytime.rs"]);
        let mut others: BTreeSet<String> = packages.iter().map(|p| p.name.clone()).collect();
        others.remove("world");
        assert!(others.len() > MAX_LOCAL_DEPENDENTS);
        assert_eq!(
            super::scope(&files, &packages, &Tree::default(), Lane::Local),
            Scope::Some {
                code: names(&["world"]),
                docs: names(&[]),
                tested: others.clone(),
                built: names(&[]),
            }
        );
        others.insert("world".to_owned());
        assert_eq!(
            super::scope(&files, &packages, &Tree::default(), Lane::Ci),
            Scope::Some {
                code: others,
                docs: names(&[]),
                tested: names(&[]),
                built: names(&[]),
            }
        );
    }

    #[test]
    fn a_change_to_an_api_crate_runs_the_luau_definitions_tests_in_every_lane() {
        // cargo-enshwick renders the Luau definitions from the script
        // descriptions the API crates register, which its tests can read
        // only by linking them through the facade: that edge is the
        // generator's input list, so the real graph reaches it with no list here.
        let packages = crate::packages().unwrap();
        let files = files(&["crates/internal/world/src/daytime.rs"]);
        for lane in [Lane::Local, Lane::Ci] {
            let Scope::Some { code, tested, .. } =
                super::scope(&files, &packages, &Tree::default(), lane)
            else {
                panic!("the whole workspace");
            };
            assert!(
                code.contains("cargo-enshwick") || tested.contains("cargo-enshwick"),
                "{lane:?}"
            );
        }
    }

    #[test]
    fn locally_dependents_up_to_the_limit_are_checked_in_full() {
        let mut packages = vec![package("base", "crates/base", &[])];
        let leaves: Vec<String> = (0..=MAX_LOCAL_DEPENDENTS)
            .map(|n| format!("mod-{n}"))
            .collect();
        for leaf in &leaves {
            packages.push(package(leaf, &format!("mods/{leaf}"), &["base"]));
        }
        let files = files(&["crates/base/src/lib.rs"]);
        let mut everyone: BTreeSet<String> = leaves.iter().cloned().collect();
        assert_eq!(
            super::scope(&files, &packages, &Tree::default(), Lane::Local),
            Scope::Some {
                code: names(&["base"]),
                docs: names(&[]),
                tested: everyone.clone(),
                built: names(&[]),
            }
        );
        everyone.insert("base".to_owned());
        assert_eq!(
            code(super::scope(&files, &packages, &Tree::default(), Lane::Ci)),
            everyone
        );
        packages.pop();
        everyone.remove(&format!("mod-{MAX_LOCAL_DEPENDENTS}"));
        assert_eq!(
            super::scope(&files, &packages, &Tree::default(), Lane::Local),
            Scope::Some {
                code: everyone,
                docs: names(&[]),
                tested: names(&[]),
                built: names(&[]),
            }
        );
    }

    #[test]
    fn a_version_bump_of_a_tested_dependent_is_tested_not_only_built() {
        let (mut files, tree) = release();
        files.push("crates/internal/macros/src/lib.rs".to_owned());
        let mut packages = workspace();
        for n in 0..MAX_LOCAL_DEPENDENTS {
            packages.push(package(
                &format!("mod-{n}"),
                &format!("mods/m{n}"),
                &["macros"],
            ));
        }
        let Scope::Some { tested, built, .. } = super::scope(&files, &packages, &tree, Lane::Local)
        else {
            panic!("the whole workspace");
        };
        assert!(tested.contains("my-mod"));
        assert!(built.is_disjoint(&tested));
    }
}
