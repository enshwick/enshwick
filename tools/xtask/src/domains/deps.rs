//! The edges of the domain graph are the ones the code uses: each internal
//! crate a domain's crate, or core, depends on is named by its code outside
//! tests, and one named only under `#[cfg(feature = "...")]` is an optional
//! dependency, so a build without that feature compiles the two side by
//! side.

use std::path::Path;

use syn::Item;

use super::{
    MAP, Map, Named, collect, dependencies, drop_test_modules, feature_only, item_attrs,
    module_path, package, paths, ungated_paths,
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
        let gated = gated_modules(&parsed);
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

/// The modules declared `#[cfg(feature = "...")] mod name;`, whose files
/// the feature turns off whole.
fn gated_modules(files: &[(String, syn::File)]) -> Vec<Vec<String>> {
    let mut gated = Vec::new();
    for (path, file) in files {
        for item in &file.items {
            if let Item::Mod(declared) = item
                && declared.content.is_none()
                && feature_only(item_attrs(item))
            {
                let mut module = module_path(path);
                module.push(declared.ident.to_string());
                gated.push(module);
            }
        }
    }
    gated
}

/// Whether the manifest's `[dependencies]` entry of `dep` is optional.
fn optional(manifest: &toml::Table, dep: &str) -> bool {
    manifest
        .get("dependencies")
        .and_then(toml::Value::as_table)
        .is_some_and(|deps| {
            deps.iter().any(|(key, entry)| {
                let package = entry
                    .get("package")
                    .and_then(toml::Value::as_str)
                    .unwrap_or(key);
                package == dep
                    && entry
                        .get("optional")
                        .and_then(toml::Value::as_bool)
                        .unwrap_or(false)
            })
        })
}

#[cfg(test)]
mod tests {
    use super::super::tests::Tree;
    use super::*;

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
        let map = Map {
            order: vec![
                "base".into(),
                "world".into(),
                "inventory".into(),
                "ui".into(),
            ],
            joins: Vec::new(),
            modules: Default::default(),
            upward: Vec::new(),
            below: Vec::new(),
            crates: [
                ("base".to_owned(), "base".to_owned()),
                ("world".to_owned(), "world".to_owned()),
                ("inventory".to_owned(), "inventory".to_owned()),
                ("ui".to_owned(), "ui".to_owned()),
            ]
            .into(),
            fake: Default::default(),
        };
        let problems = problems(&root, &map).unwrap();
        assert_eq!(problems.len(), 3, "{problems:?}");
        assert!(problems[0].contains("enshwick-ui names enshwick-base only under a cfg(feature)"));
        assert!(problems[1].contains("enshwick-ui names enshwick-world only under a cfg(feature)"));
        assert!(
            problems[2]
                .contains("enshwick-core depends on enshwick-base, which its code names nowhere")
        );
    }
}
