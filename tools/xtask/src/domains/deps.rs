//! The edges of the domain graph are the ones the code uses: each internal
//! crate a domain's crate, or core, depends on is named by its code outside
//! tests, and one named only under `#[cfg(feature = "...")]` is an optional
//! dependency, so a build without that feature compiles the two side by
//! side.

use std::collections::BTreeSet;
use std::path::Path;

use super::walk::{Named, feature_only, item_attrs, ungated_paths};
use super::{MAP, Map, collect, declared_modules, drop_test_modules, module_path, package, paths};

/// The crate above every domain, by its folder under `crates/internal`.
const CORE: &str = "core";

/// The document that draws the domains' graph, from the workspace root.
const GRAPH: &str = "ARCHITECTURE.md";

/// One internal crate a manifest depends on.
struct Dep {
    /// Its package name.
    package: String,
    /// The name the code writes it by: the manifest's key, `-` as `_`.
    ident: String,
    optional: bool,
}

/// A dependency between two domains, by their folders; `optional` draws it
/// dotted.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Edge {
    from: String,
    to: String,
    optional: bool,
}

/// The internal crates each domain's crate and core depend on but do not
/// use as their manifest says, and the drawn graph if it differs from the
/// manifests, one line each.
pub(super) fn problems(root: &Path, map: &Map) -> Vec<String> {
    let (mut problems, edges) = manifests(root, map);
    // A tree with no domain crates draws no graph.
    if map.crates.is_empty() {
        return problems;
    }
    match std::fs::read_to_string(root.join(GRAPH)) {
        Ok(text) => problems.extend(graph(&text, &edges)),
        Err(e) => problems.push(format!("{GRAPH}: {e}")),
    }
    problems
}

/// The problems of [`problems`] in the manifests and the code, with the
/// edges between domains the manifests make.
fn manifests(root: &Path, map: &Map) -> (Vec<String>, Vec<Edge>) {
    let mut problems = Vec::new();
    let mut packages = Vec::new();
    for dir in map.crates.values().map(String::as_str).chain([CORE]) {
        match package(root, dir) {
            Ok((manifest, name)) => packages.push((dir, manifest, name)),
            Err(e) => problems.push(e),
        }
    }
    let internal: Vec<&str> = map
        .below
        .iter()
        .chain(packages.iter().map(|(_, _, name)| name))
        .map(String::as_str)
        .collect();
    let domain_of = |package: &str| {
        packages
            .iter()
            .find(|(dir, _, name)| *dir != CORE && name == package)
            .map(|(dir, _, _)| (*dir).to_owned())
    };
    let mut edges = Vec::new();
    for (dir, manifest, name) in &packages {
        let deps: Vec<Dep> = dependencies(manifest)
            .filter(|dep| internal.contains(&dep.package.as_str()))
            .collect();
        let files = match sources(root, dir) {
            Ok(files) => files,
            Err(e) => {
                problems.push(e);
                continue;
            }
        };
        let idents: Vec<&str> = deps.iter().map(|dep| dep.ident.as_str()).collect();
        let (always, named) = usage(&files, &idents);
        let at = format!("crates/internal/{dir}/Cargo.toml: {name}");
        for Dep {
            package,
            ident,
            optional,
        } in &deps
        {
            if !named.contains(ident) {
                problems.push(format!(
                    "{at} depends on {package}, which its code names nowhere outside tests: drop it, or make it a dev-dependency ({MAP})"
                ));
            } else if !always.contains(ident) && !optional {
                problems.push(format!(
                    "{at} names {package} only under a cfg(feature): make it optional and turn it on with that feature ({MAP})"
                ));
            }
            if *dir != CORE
                && let Some(to) = domain_of(package)
            {
                edges.push(Edge {
                    from: (*dir).to_owned(),
                    to,
                    optional: *optional,
                });
            }
        }
    }
    (problems, edges)
}

/// The parsed files of a crate's code outside tests.
fn sources(root: &Path, dir: &str) -> Result<Vec<(String, syn::File)>, String> {
    let src = root.join("crates/internal").join(dir).join("src");
    let mut files = Vec::new();
    collect(&src, &src, &mut files)?;
    drop_test_modules(&mut files);
    files
        .into_iter()
        .map(|(path, text)| {
            syn::parse_file(&text)
                .map(|file| (path.clone(), file))
                .map_err(|e| format!("crates/internal/{dir}/src/{path}: {e}"))
        })
        .collect()
}

/// Of `idents`, the crates the code names where no feature turns it off,
/// and those it names at all.
fn usage(files: &[(String, syn::File)], idents: &[&str]) -> (BTreeSet<String>, BTreeSet<String>) {
    let gated = declared_modules(files, |item| feature_only(item_attrs(item)));
    let firsts = |named: Named| {
        named
            .external
            .into_iter()
            .filter_map(|path| path.into_iter().next())
    };
    let (mut always, mut named) = (BTreeSet::new(), BTreeSet::new());
    for (path, file) in files {
        let module = module_path(path);
        if !feature_only(&file.attrs) && !gated.iter().any(|g| module.starts_with(g)) {
            always.extend(firsts(ungated_paths(file, &module, idents)));
        }
        named.extend(firsts(paths(file, &module, idents)));
    }
    (always, named)
}

/// The manifest's dependencies, in `[dependencies]` and every
/// `[target.*.dependencies]`.
fn dependencies(manifest: &toml::Table) -> impl Iterator<Item = Dep> {
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
        .map(|(key, entry)| Dep {
            package: entry
                .get("package")
                .and_then(toml::Value::as_str)
                .unwrap_or(key)
                .to_owned(),
            ident: key.replace('-', "_"),
            optional: entry
                .get("optional")
                .and_then(toml::Value::as_bool)
                .unwrap_or(false),
        })
}

/// The line of a problem when the graph drawn in [`GRAPH`]'s mermaid block
/// is not the one `edges` make, an edge a longer path gives left out: a
/// solid one when solid edges give that path, a dotted one when any do.
fn graph(text: &str, edges: &[Edge]) -> Option<String> {
    let Some(block) = text
        .split("```mermaid\n")
        .nth(1)
        .and_then(|rest| rest.split("```").next())
    else {
        return Some(format!(
            "{GRAPH}: no mermaid block draws the domains' graph ({MAP})"
        ));
    };
    let drawn: BTreeSet<Edge> = block
        .lines()
        .filter_map(|line| {
            let (from, to, optional) = if let Some((from, to)) = line.split_once("-->") {
                (from, to, false)
            } else {
                let (from, rest) = line.split_once("-.")?;
                (from, rest.split_once(".->")?.1, true)
            };
            Some(Edge {
                from: from.trim().to_owned(),
                to: to.trim().to_owned(),
                optional,
            })
        })
        .collect();
    let reaches = |from: &str, to: &str, solid_only: bool| {
        let mut todo: Vec<&str> = edges
            .iter()
            .filter(|edge| edge.from == from && edge.to != to && !(solid_only && edge.optional))
            .map(|edge| edge.to.as_str())
            .collect();
        let mut seen = BTreeSet::new();
        while let Some(at) = todo.pop() {
            if at == to {
                return true;
            }
            if seen.insert(at) {
                todo.extend(
                    edges
                        .iter()
                        .filter(|edge| edge.from == at && !(solid_only && edge.optional))
                        .map(|edge| edge.to.as_str()),
                );
            }
        }
        false
    };
    let made: BTreeSet<Edge> = edges
        .iter()
        .filter(|edge| !reaches(&edge.from, &edge.to, !edge.optional))
        .cloned()
        .collect();
    let line = |edge: &Edge| {
        let arrow = if edge.optional { "-. .->" } else { "-->" };
        format!("{} {arrow} {}", edge.from, edge.to)
    };
    let missing: Vec<String> = made.difference(&drawn).map(line).collect();
    let stray: Vec<String> = drawn.difference(&made).map(line).collect();
    (!missing.is_empty() || !stray.is_empty()).then(|| {
        format!(
            "{GRAPH}: the domains' graph misses [{}] and draws [{}], which the manifests do not make ({MAP})",
            missing.join(", "),
            stray.join(", ")
        )
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
            loader: super::super::loader::Loader::default(),
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
        let (problems, _) = manifests(&root, &map(&dirs));
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
    fn a_dependency_reads_its_package_code_name_and_optional_in_a_target_table_too()
    -> Result<(), toml::de::Error> {
        let manifest: toml::Table = toml::from_str(
            "[dependencies]\nenshwick-base.workspace = true\nup = { package = \"enshwick-ui\", optional = true }\n\n[target.'cfg(windows)'.dependencies]\nenshwick-world = { workspace = true, optional = true }\n",
        )?;
        let deps: Vec<(String, String, bool)> = dependencies(&manifest)
            .map(|dep| (dep.package, dep.ident, dep.optional))
            .collect();
        let dep =
            |package: &str, ident: &str, optional| (package.to_owned(), ident.to_owned(), optional);
        assert_eq!(
            deps,
            [
                dep("enshwick-base", "enshwick_base", false),
                dep("enshwick-ui", "up", true),
                dep("enshwick-world", "enshwick_world", true),
            ]
        );
        Ok(())
    }

    #[test]
    fn a_renamed_dependency_a_derive_path_and_test_only_impl_items_count_by_what_the_code_names() {
        let root = std::env::temp_dir().join(format!("xtask-deps-names-{}", std::process::id()));
        let _gone = std::fs::remove_dir_all(&root);
        let tree = Tree(root.clone());
        for dir in ["base", "world", "inventory", "core"] {
            tree.write(
                &format!("crates/internal/{dir}/Cargo.toml"),
                &format!("[package]\nname = \"enshwick-{dir}\"\n"),
            );
            tree.write(&format!("crates/internal/{dir}/src/lib.rs"), "");
        }
        tree.write(
            "crates/internal/ui/Cargo.toml",
            "[package]\nname = \"enshwick-ui\"\n[dependencies]\nlow = { package = \"enshwick-base\", workspace = true }\nenshwick-world.workspace = true\nenshwick-inventory.workspace = true\n",
        );
        // base by its new name, world only in a derive, inventory only in a
        // test helper of an ordinary impl.
        tree.write(
            "crates/internal/ui/src/lib.rs",
            "use low::x;\n#[derive(enshwick_world::Shape)]\nstruct S;\nimpl S {\n    #[cfg(test)]\n    fn fixture() { enshwick_inventory::f(); }\n}\n",
        );
        let (problems, _) = manifests(&root, &map(&["base", "world", "inventory", "ui"]));
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(
            problems[0].contains(
                "enshwick-ui depends on enshwick-inventory, which its code names nowhere"
            ),
            "{problems:?}"
        );
    }

    #[test]
    fn a_domain_without_its_folder_is_one_problem_and_the_others_are_still_read() {
        let root = std::env::temp_dir().join(format!("xtask-deps-gone-{}", std::process::id()));
        let _gone = std::fs::remove_dir_all(&root);
        let tree = Tree(root.clone());
        for dir in ["base", "core"] {
            tree.write(
                &format!("crates/internal/{dir}/Cargo.toml"),
                &format!("[package]\nname = \"enshwick-{dir}\"\n[dependencies]\nenshwick-abi.workspace = true\n"),
            );
            tree.write(&format!("crates/internal/{dir}/src/lib.rs"), "");
        }
        let mut map = map(&["base", "sound"]);
        map.below = vec!["enshwick-abi".to_owned()];
        let (problems, _) = manifests(&root, &map);
        assert_eq!(problems.len(), 3, "{problems:?}");
        assert!(
            problems[0].starts_with("crates/internal/sound/Cargo.toml"),
            "{problems:?}"
        );
        assert!(
            problems[1].contains("enshwick-base depends on enshwick-abi"),
            "{problems:?}"
        );
        assert!(
            problems[2].contains("enshwick-core depends on enshwick-abi"),
            "{problems:?}"
        );
    }

    #[test]
    fn the_drawn_graph_must_be_the_manifests_edges_without_those_a_longer_path_gives() {
        let edge = |from: &str, to: &str, optional| Edge {
            from: from.to_owned(),
            to: to.to_owned(),
            optional,
        };
        // ui -> base is given by ui -> world -> base only with unstable, so
        // it stays; knowledge -> base is given by solid edges, so it goes.
        let edges = [
            edge("world", "base", false),
            edge("ui", "world", true),
            edge("ui", "base", false),
            edge("knowledge", "world", false),
            edge("knowledge", "base", false),
        ];
        let drawn = "```mermaid\ngraph BT\n    world --> base\n    ui -. unstable .-> world\n    ui --> base\n    knowledge --> world\n```\n";
        assert_eq!(graph(drawn, &edges), None);
        let wrong = drawn.replace("    ui --> base\n", "    knowledge --> base\n");
        let problem = graph(&wrong, &edges).unwrap();
        assert!(
            problem.contains("misses [ui --> base] and draws [knowledge --> base]"),
            "{problem}"
        );
        assert!(
            graph("no graph", &edges)
                .unwrap()
                .contains("no mermaid block")
        );
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
        let (problems, _) = manifests(&root, &map(&["base", "world", "inventory", "ui"]));
        assert_eq!(problems.len(), 3, "{problems:?}");
        assert!(problems[0].contains("enshwick-ui names enshwick-base only under a cfg(feature)"));
        assert!(problems[1].contains("enshwick-ui names enshwick-world only under a cfg(feature)"));
        assert!(
            problems[2]
                .contains("enshwick-core depends on enshwick-base, which its code names nowhere")
        );
    }
}
