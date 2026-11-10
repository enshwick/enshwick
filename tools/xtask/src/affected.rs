//! Which packages a change touches: the packages whose files changed and
//! every workspace package that depends on them.

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
}

/// What a change needs checked.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Scope {
    /// A file every package builds with changed: check the whole workspace.
    All,
    /// Only these packages.
    Some {
        /// Packages to lint and test: changed ones and their dependents.
        code: BTreeSet<String>,
        /// Packages whose doctests changed without their code: the book's
        /// chapters are doctests of the API crate.
        docs: BTreeSet<String>,
    },
}

/// Files that every package builds with.
const GLOBAL_FILES: &[&str] = &[
    "Cargo.toml",
    "Cargo.lock",
    "rust-toolchain.toml",
    "clippy.toml",
];
const GLOBAL_DIRS: &[&str] = &[".cargo/", ".config/"];
/// The book's chapters, compiled as doctests of the API crate.
const BOOK_DIR: &str = "book/src/";
const BOOK_HARNESS: &str = "enshwick";
/// The package whose tests compile and type-check the book's Luau examples.
const BOOK_LUAU: &str = "enshwick-luau";

/// Sorts changed files (relative to the root, `/` separators) into a scope.
pub(crate) fn scope(files: &[String], packages: &[Package]) -> Scope {
    let mut changed = BTreeSet::new();
    let mut docs = BTreeSet::new();
    let mut luau = false;
    for file in files {
        if GLOBAL_FILES.contains(&file.as_str())
            || GLOBAL_DIRS.iter().any(|dir| file.starts_with(dir))
        {
            return Scope::All;
        }
        if file.starts_with(BOOK_DIR) {
            docs.insert(BOOK_HARNESS.to_owned());
            luau = true;
        } else if let Some(package) = owner(file, packages) {
            changed.insert(package.name.clone());
        }
    }
    let mut code = dependents(&changed, packages);
    // Its own tests read the chapters and run luau-lsp; what depends on it does not.
    if luau && packages.iter().any(|package| package.name == BOOK_LUAU) {
        code.insert(BOOK_LUAU.to_owned());
    }
    docs.retain(|name| !code.contains(name));
    Scope::Some { code, docs }
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
        }
    }

    fn workspace() -> Vec<Package> {
        vec![
            package("macros", "crates/enshwick-macros", &[]),
            package("enshwick", "crates/enshwick", &["macros"]),
            package("loader", "crates/internal/loader", &["enshwick"]),
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

    #[test]
    fn a_mod_change_checks_the_mod_and_its_dependents_only() {
        let scope = scope(&files(&["mods/trading-post/src/lib.rs"]), &workspace());
        assert_eq!(
            scope,
            Scope::Some {
                code: names(&["trading-post", "haggling"]),
                docs: names(&[]),
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
    fn a_shared_build_file_checks_everything() {
        for file in [
            "Cargo.lock",
            "Cargo.toml",
            "rust-toolchain.toml",
            ".cargo/config.toml",
            ".config/nextest.toml",
        ] {
            assert_eq!(scope(&files(&[file]), &workspace()), Scope::All, "{file}");
        }
    }

    #[test]
    fn a_book_chapter_runs_only_the_api_doctests() {
        assert_eq!(
            scope(&files(&["book/src/hooks.md"]), &workspace()),
            Scope::Some {
                code: names(&[]),
                docs: names(&["enshwick"]),
            }
        );
    }

    #[test]
    fn a_book_chapter_also_tests_the_luau_runtime_that_checks_its_examples() {
        let mut packages = workspace();
        packages.push(package(
            "enshwick-luau",
            "crates/internal/luau",
            &["enshwick"],
        ));
        packages.push(package(
            "loader-on-luau",
            "crates/internal/loader2",
            &["enshwick-luau"],
        ));
        assert_eq!(
            scope(&files(&["book/src/luau.md"]), &packages),
            Scope::Some {
                code: names(&["enshwick-luau"]),
                docs: names(&["enshwick"]),
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
            "crates/internal/luau",
            &["enshwick", "cargo-enshwick"],
        ));
        assert_eq!(
            scope(&files(&["crates/cargo-enshwick/luau-lsp.toml"]), &packages),
            Scope::Some {
                code: names(&["cargo-enshwick", "enshwick-luau"]),
                docs: names(&[]),
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
}
