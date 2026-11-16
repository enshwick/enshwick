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
    let tree = Tree::changed(shared::LOCK, &lock("bb"), &lock("b2"));
    assert_eq!(
        code(super::scope(
            &files(&[shared::LOCK]),
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
        shared::LOCK,
        &lock("bb"),
        &lock("bb").replace("checksum = \"aa\"", "checksum = \"a2\""),
    );
    assert_eq!(
        code(super::scope(
            &files(&[shared::LOCK]),
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
    let tree = Tree::changed(shared::LOCK, &lock("bb"), "[[package]");
    assert_eq!(
        super::scope(&files(&[shared::LOCK]), &workspace(), &tree, Lane::Ci),
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
        &manifest("[workspace.dependencies]\nwin = { package = \"windows\", version = \"0.1\" }\n"),
        &manifest(
            "[workspace.dependencies]\nwin = { package = \"windows\", version = \"0.1\", features = [\"Win32\"] }\n",
        ),
    );
    tree.after.insert(shared::LOCK.to_owned(), lock("bb"));
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
            wide: false,
        }
    );
}

#[test]
fn an_api_change_reaches_every_package_built_on_it() {
    let Scope::Some { code, .. } = scope(&files(&["crates/enshwick/src/load.rs"]), &workspace())
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
            wide: false,
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
            wide: false,
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
            wide: false,
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
            wide: false,
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
                wide: false,
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
            wide: false,
        }
    );
}

/// A release's change: every member's version, the root's path entries
/// and the lock's member versions move from `alpha.14` to `alpha.15`.
fn release() -> (Vec<String>, Tree) {
    let mut tree = Tree::default();
    let mut changed = vec!["Cargo.toml".to_owned(), shared::LOCK.to_owned()];
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
    for (path, text) in [("Cargo.toml", root), (shared::LOCK, lock)] {
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
            wide: false,
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
            wide: false,
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
    let Scope::Some { code, built, .. } = super::scope(&files, &workspace(), &tree, Lane::Local)
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
    assert!(others.len() >= MAX_LOCAL_PACKAGES);
    assert_eq!(
        super::scope(&files, &packages, &Tree::default(), Lane::Local),
        Scope::Some {
            code: names(&["world"]),
            docs: names(&[]),
            tested: others.clone(),
            built: names(&[]),
            wide: true,
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
            wide: false,
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
fn locally_a_change_reaching_up_to_the_limit_is_checked_in_full() {
    let (mut packages, leaves) = base_and_leaves();
    let files = files(&["crates/base/src/lib.rs"]);
    let mut everyone: BTreeSet<String> = leaves.iter().cloned().collect();
    assert_eq!(
        super::scope(&files, &packages, &Tree::default(), Lane::Local),
        Scope::Some {
            code: names(&["base"]),
            docs: names(&[]),
            tested: everyone.clone(),
            built: names(&[]),
            wide: true,
        }
    );
    everyone.insert("base".to_owned());
    assert_eq!(
        code(super::scope(&files, &packages, &Tree::default(), Lane::Ci)),
        everyone
    );
    packages.pop();
    everyone.remove(leaves.last().unwrap());
    assert_eq!(
        super::scope(&files, &packages, &Tree::default(), Lane::Local),
        Scope::Some {
            code: everyone,
            docs: names(&[]),
            tested: names(&[]),
            built: names(&[]),
            wide: false,
        }
    );
}

/// A `base` package and one more leaf depending on it than a change to
/// `base` may reach locally and still be checked in full.
fn base_and_leaves() -> (Vec<Package>, Vec<String>) {
    let mut packages = vec![package("base", "crates/base", &[])];
    let leaves: Vec<String> = (0..MAX_LOCAL_PACKAGES)
        .map(|n| format!("mod-{n}"))
        .collect();
    for leaf in &leaves {
        packages.push(package(leaf, &format!("mods/{leaf}"), &["base"]));
    }
    (packages, leaves)
}

#[test]
fn locally_a_change_to_many_packages_with_few_dependents_is_wide() {
    let (packages, leaves) = base_and_leaves();
    let changed: Vec<String> = packages
        .iter()
        .map(|package| format!("{}/src/lib.rs", package.dir))
        .collect();
    let everyone: BTreeSet<String> = packages.iter().map(|p| p.name.clone()).collect();
    assert_eq!(
        super::scope(&changed, &packages, &Tree::default(), Lane::Local),
        Scope::Some {
            code: everyone.clone(),
            docs: names(&[]),
            tested: names(&[]),
            built: names(&[]),
            wide: true,
        }
    );
    let leaf_files: Vec<String> = leaves
        .iter()
        .map(|leaf| format!("mods/{leaf}/src/lib.rs"))
        .collect();
    let Scope::Some { code, wide, .. } =
        super::scope(&leaf_files, &packages, &Tree::default(), Lane::Local)
    else {
        panic!("the whole workspace");
    };
    assert_eq!(code.len(), MAX_LOCAL_PACKAGES);
    assert!(!wide, "{code:?}");
    assert!(matches!(
        super::scope(&changed, &packages, &Tree::default(), Lane::Ci),
        Scope::Some { wide: false, .. }
    ));
}

#[test]
fn locally_the_luau_runtime_a_book_chapter_adds_counts_towards_the_limit() {
    let (mut packages, leaves) = base_and_leaves();
    packages.pop();
    packages.push(package(BOOK_LUAU, "crates/runtime/luau", &[]));
    let files = files(&["crates/base/src/lib.rs", "book/src/luau.md"]);
    let Scope::Some {
        code, tested, wide, ..
    } = super::scope(&files, &packages, &Tree::default(), Lane::Local)
    else {
        panic!("the whole workspace");
    };
    assert!(wide);
    assert_eq!(code, names(&["base", BOOK_LUAU]));
    assert_eq!(tested.len(), leaves.len() - 1);
}

#[test]
fn a_version_bump_of_a_tested_dependent_is_tested_not_only_built() {
    let (mut files, tree) = release();
    files.push("crates/internal/macros/src/lib.rs".to_owned());
    let mut packages = workspace();
    for n in 0..MAX_LOCAL_PACKAGES {
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
