//! The edges of the domain graph are the ones the code uses: each internal
//! crate a domain's crate, or core, depends on is named by its code outside
//! tests, and one named only under `#[cfg(feature = "...")]` is an optional
//! dependency, so a build without that feature compiles the two side by
//! side.

use std::path::Path;

use super::{
    MAP, Map, Named, collect, declared_modules, dependencies, drop_test_modules, feature_only,
    item_attrs, module_path, package, paths, ungated_paths,
};

/// The crate above every domain, by its folder under `crates/internal`.
const CORE: &str = "core";

/// Where an internal crate a manifest depends on is named in its crate's
/// code outside tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Use {
    /// Nowhere.
    None,
    /// Only under a `cfg(feature)`.
    Gated,
    /// Somewhere no feature turns off.
    Always,
}

/// The internal crates each domain's crate and core depend on but do not
/// use as their manifest says, one line each.
pub(super) fn problems(root: &Path, map: &Map) -> Result<Vec<String>, String> {
    let mut internal = map.below.clone();
    for dir in map.crates.values().map(String::as_str).chain([CORE]) {
        internal.push(package(root, dir)?.1);
    }
    let mut problems = Vec::new();
    for dir in map.crates.values().map(String::as_str).chain([CORE]) {
        let (manifest, name) = package(root, dir)?;
        let deps: Vec<String> = dependencies(&manifest)
            .into_iter()
            .filter(|dep| internal.contains(dep))
            .collect();
        let src = root.join("crates/internal").join(dir).join("src");
        let mut files = Vec::new();
        collect(&src, &src, &mut files)?;
        drop_test_modules(&mut files);
        let mut parsed = Vec::new();
        for (path, text) in &files {
            let file = syn::parse_file(text)
                .map_err(|e| format!("crates/internal/{dir}/src/{path}: {e}"))?;
            parsed.push((path.clone(), file));
        }
        let gated = declared_modules(&parsed, |item| feature_only(item_attrs(item)));
        for dep in &deps {
            let usage = usage(&parsed, &gated, &dep.replace('-', "_"));
            let at = format!("crates/internal/{dir}/Cargo.toml: {name}");
            match usage {
                Use::None => problems.push(format!(
                    "{at} depends on {dep}, which its code names nowhere outside tests: drop it, or make it a dev-dependency ({MAP})"
                )),
                Use::Gated if !optional(&manifest, dep) => problems.push(format!(
                    "{at} names {dep} only under a cfg(feature): make it optional and turn it on with that feature ({MAP})"
                )),
                Use::Gated | Use::Always => {}
            }
        }
    }
    Ok(problems)
}

/// Where the files of a crate name `ident`, the crate as its code writes
/// it; `gated` lists the modules declared under a `cfg(feature)`.
fn usage(files: &[(String, syn::File)], gated: &[Vec<String>], ident: &str) -> Use {
    let externs = [ident];
    let mut found = Use::None;
    for (path, file) in files {
        let module = module_path(path);
        let names = |named: Named| {
            named
                .external
                .iter()
                .any(|path| path.first().is_some_and(|first| first == ident))
        };
        let whole_file_gated =
            feature_only(&file.attrs) || gated.iter().any(|g| module.starts_with(g));
        if !whole_file_gated && names(ungated_paths(file, &module, &externs)) {
            return Use::Always;
        }
        if names(paths(file, &module, &externs)) {
            found = Use::Gated;
        }
    }
    found
}

/// Whether the manifest's entry of `dep`, in `[dependencies]` or a
/// `[target.*.dependencies]`, is optional.
fn optional(manifest: &toml::Table, dep: &str) -> bool {
    let targets = manifest
        .get("target")
        .and_then(toml::Value::as_table)
        .into_iter()
        .flat_map(|targets| targets.values());
    std::iter::once(manifest.get("dependencies"))
        .chain(targets.map(|target| target.get("dependencies")))
        .flatten()
        .filter_map(toml::Value::as_table)
        .flatten()
        .any(|(key, entry)| {
            entry
                .get("package")
                .and_then(toml::Value::as_str)
                .unwrap_or(key)
                == dep
                && entry
                    .get("optional")
                    .and_then(toml::Value::as_bool)
                    .unwrap_or(false)
        })
}

#[cfg(test)]
mod tests {
    use super::super::tests::Tree;
    use super::*;

    /// A map whose domains, in order, each live in the folder of their name.
    fn map(dirs: &[&str]) -> Map {
        Map {
            order: dirs.iter().map(|dir| (*dir).to_owned()).collect(),
            joins: Vec::new(),
            modules: std::collections::BTreeMap::new(),
            upward: Vec::new(),
            below: Vec::new(),
            crates: dirs
                .iter()
                .map(|dir| ((*dir).to_owned(), (*dir).to_owned()))
                .collect(),
            fake: super::super::fake::Fake::default(),
        }
    }

    #[test]
    fn a_compound_feature_cfg_a_gated_statement_field_or_arm_and_a_gated_module_in_an_inline_module_count_as_gated()
     {
        let root = std::env::temp_dir().join(format!("xtask-deps-gates-{}", std::process::id()));
        let _gone = std::fs::remove_dir_all(&root);
        let tree = Tree(root.clone());
        let below = [
            "base",
            "content",
            "world",
            "inventory",
            "knowledge",
            "actions",
        ];
        for dir in below {
            tree.write(
                &format!("crates/internal/{dir}/Cargo.toml"),
                &format!("[package]\nname = \"enshwick-{dir}\"\n"),
            );
            tree.write(&format!("crates/internal/{dir}/src/lib.rs"), "");
        }
        let deps = below
            .map(|dir| format!("enshwick-{dir}.workspace = true\n"))
            .concat();
        tree.write(
            "crates/internal/ui/Cargo.toml",
            &format!("[package]\nname = \"enshwick-ui\"\n[dependencies]\n{deps}"),
        );
        tree.write(
            "crates/internal/core/Cargo.toml",
            "[package]\nname = \"enshwick-core\"\n",
        );
        tree.write("crates/internal/core/src/lib.rs", "");
        // base under all(feature, ..), content in a gated statement, world
        // in a gated field, inventory in a gated arm, knowledge in a gated
        // module inside an inline one: each needs `optional`. actions under
        // any(test, feature) builds in tests without the feature, so it is
        // used always.
        tree.write(
            "crates/internal/ui/src/lib.rs",
            "#[cfg(all(feature = \"unstable\", windows))]\n\
             pub use enshwick_base::x;\n\
             fn f(n: u8) {\n\
                 #[cfg(feature = \"unstable\")]\n\
                 enshwick_content::g();\n\
                 match n {\n\
                     #[cfg(feature = \"unstable\")]\n\
                     0 => enshwick_inventory::h(),\n\
                     _ => {}\n\
                 }\n\
             }\n\
             struct S {\n\
                 #[cfg(feature = \"unstable\")]\n\
                 w: enshwick_world::W,\n\
             }\n\
             mod outer {\n\
                 #[cfg(feature = \"unstable\")]\n\
                 mod quests;\n\
             }\n\
             #[cfg(any(test, feature = \"fake\"))]\n\
             pub use enshwick_actions::y;\n",
        );
        tree.write(
            "crates/internal/ui/src/outer/quests.rs",
            "use enshwick_knowledge::Q;",
        );
        let mut dirs = below.to_vec();
        dirs.push("ui");
        let problems = problems(&root, &map(&dirs)).unwrap();
        let mut gated: Vec<&str> = problems
            .iter()
            .filter_map(|problem| {
                problem
                    .strip_prefix("crates/internal/ui/Cargo.toml: enshwick-ui names ")?
                    .split_once(' ')
                    .map(|(dep, _)| dep)
            })
            .collect();
        gated.sort_unstable();
        assert_eq!(
            gated,
            [
                "enshwick-base",
                "enshwick-content",
                "enshwick-inventory",
                "enshwick-knowledge",
                "enshwick-world"
            ],
            "{problems:?}"
        );
        assert_eq!(problems.len(), 5, "{problems:?}");
    }

    #[test]
    fn a_dependency_counts_as_optional_in_a_target_table_too() -> Result<(), toml::de::Error> {
        let manifest: toml::Table = toml::from_str(
            "[dependencies]\nenshwick-base.workspace = true\nup = { package = \"enshwick-ui\", optional = true }\n\n[target.'cfg(windows)'.dependencies]\nenshwick-world = { workspace = true, optional = true }\n",
        )?;
        assert!(!optional(&manifest, "enshwick-base"));
        assert!(optional(&manifest, "enshwick-ui"));
        assert!(optional(&manifest, "enshwick-world"));
        assert!(!optional(&manifest, "enshwick-content"));
        Ok(())
    }

    #[test]
    fn a_dependency_named_nowhere_in_tests_only_or_only_under_a_feature_without_optional_fails() {
        let root = std::env::temp_dir().join(format!("xtask-deps-{}", std::process::id()));
        let _gone = std::fs::remove_dir_all(&root);
        let tree = Tree(root.clone());
        let manifest = |dir: &str, deps: &str| {
            tree.write(
                &format!("crates/internal/{dir}/Cargo.toml"),
                &format!("[package]\nname = \"enshwick-{dir}\"\n[dependencies]\n{deps}"),
            );
        };
        for dir in ["base", "world", "inventory"] {
            manifest(dir, "");
            tree.write(&format!("crates/internal/{dir}/src/lib.rs"), "");
        }
        manifest(
            "ui",
            "enshwick-base.workspace = true\nenshwick-world.workspace = true\nenshwick-inventory = { workspace = true, optional = true }\nserde = \"1\"\n",
        );
        manifest(
            "core",
            "enshwick-base.workspace = true\nenshwick-ui.workspace = true\n",
        );
        // base under a feature, world in a module a feature turns off, and
        // inventory, optional, under a feature too: only world and base
        // need `optional`.
        tree.write(
            "crates/internal/ui/src/lib.rs",
            "#[cfg(feature = \"unstable\")]\npub use enshwick_base::x;\n#[cfg(feature = \"unstable\")]\nmod map;\nimpl A { #[cfg(feature = \"unstable\")] fn f() { enshwick_inventory::g(); } }",
        );
        tree.write("crates/internal/ui/src/map.rs", "use enshwick_world::Map;");
        // core names ui always and base only in a test.
        tree.write(
            "crates/internal/core/src/lib.rs",
            "pub use enshwick_ui::ui;\n#[cfg(test)]\nmod tests { use enshwick_base::x; }",
        );
        let problems = problems(&root, &map(&["base", "world", "inventory", "ui"])).unwrap();
        assert_eq!(problems.len(), 3, "{problems:?}");
        assert!(problems[0].contains("enshwick-ui names enshwick-base only under a cfg(feature)"));
        assert!(problems[1].contains("enshwick-ui names enshwick-world only under a cfg(feature)"));
        assert!(
            problems[2]
                .contains("enshwick-core depends on enshwick-base, which its code names nowhere")
        );
    }
}
