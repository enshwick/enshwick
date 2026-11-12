//! The domain rule in the loader: each file under `src/<layer>/` belongs to
//! that layer, the layers below the domains first and then the domains in
//! their order, and each file under `src/host/entries/<domain>` to that
//! domain; the joins, `lib.rs`, `host`, `start` and `table_ids`, sit above
//! them all. A layer's modules name only their own layer and those before
//! it, and no join; the edges up still in the code are listed, module by
//! module, and may only go.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use syn::Item;

use super::{MAP, Map, collect, drop_test_modules, listed_problem, module_path, paths, use_leaves};

/// The loader's sources, from the workspace root.
pub(super) const SOURCES: &str = "crates/runtime/loader/src";

/// The `[loader]` table of `domains.toml`.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Loader {
    /// The layers below every domain, lowest first.
    below: Vec<String>,
    /// The top-level modules under `src` that join every layer.
    joins: Vec<String>,
    /// The folder under `src` whose files are cut by the domains as well.
    entries: String,
    /// The edges up the order still in the code, as `from -> to` between
    /// modules, each `<layer>::<module>` or a join.
    upward: Vec<String>,
}

/// Where a file or a path of the loader sits: its layer's rank and the
/// module an edge names.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Place {
    rank: usize,
    module: String,
}

/// The loader's layers, lowest first, and its joins, above them.
struct Layers<'a> {
    order: Vec<&'a str>,
    loader: &'a Loader,
}

impl Layers<'_> {
    /// The place of the absolute path `abs`, `None` for one outside the
    /// layers and joins or one that names a layer alone.
    fn of(&self, abs: &[String]) -> Option<Place> {
        let first = abs.first()?.as_str();
        let entries: Vec<&str> = self.loader.entries.split('/').collect();
        if abs.len() > entries.len() && abs.iter().zip(&entries).all(|(a, b)| a == b) {
            let domain = abs[entries.len()].as_str();
            let rank = self.order.iter().position(|l| *l == domain)?;
            return Some(Place {
                rank,
                module: format!("{}::{domain}", entries.last().unwrap_or(&"")),
            });
        }
        if self.loader.joins.iter().any(|join| join == first) {
            return Some(Place {
                rank: self.order.len(),
                module: first.to_owned(),
            });
        }
        let rank = self.order.iter().position(|l| *l == first)?;
        let module = abs.get(1)?;
        Some(Place {
            rank,
            module: format!("{first}::{module}"),
        })
    }

    /// The place of a file under `src`, by its module path.
    fn of_file(&self, path: &str) -> Result<Place, String> {
        let module = module_path(path);
        if module.is_empty() {
            return Ok(Place {
                rank: self.order.len(),
                module: "lib".to_owned(),
            });
        }
        self.of(&module)
            .or_else(|| {
                // A layer's own file declares its modules and names nothing.
                let layer = module.first()?;
                (module.len() == 1)
                    .then(|| self.order.iter().position(|l| l == layer))
                    .flatten()
                    .map(|rank| Place {
                        rank,
                        module: layer.clone(),
                    })
            })
            .ok_or_else(|| {
                format!("{SOURCES}/{path} is in no layer of {MAP}: move it into a layer's folder")
            })
    }
}

/// What breaks the domain rule in the loader under `root`, one line each.
pub(super) fn problems(root: &Path, map: &Map) -> Result<Vec<String>, String> {
    let sources = root.join(SOURCES);
    if !sources.is_dir() {
        return Ok(Vec::new());
    }
    let loader = &map.loader;
    let layers = Layers {
        order: loader
            .below
            .iter()
            .chain(&map.order)
            .map(String::as_str)
            .collect(),
        loader,
    };
    let mut files = Vec::new();
    collect(&sources, &sources, &mut files)?;
    drop_test_modules(&mut files);
    files.sort();
    let mut problems = Vec::new();
    let mut parsed = Vec::new();
    for (path, text) in &files {
        let file = syn::parse_file(text).map_err(|e| format!("{SOURCES}/{path}: {e}"))?;
        parsed.push((path.as_str(), file));
    }
    let roots = parsed
        .iter()
        .find(|(path, _)| *path == "lib.rs")
        .map(|(_, lib)| root_exports(lib))
        .unwrap_or_default();
    let home = loader.entries.split('/').next();
    let entry = format!(
        "{}::",
        loader.entries.rsplit('/').next().unwrap_or_default()
    );
    let mut edges = BTreeMap::new();
    let mut down = BTreeSet::new();
    for (path, file) in &parsed {
        let (path, file) = (*path, file);
        let from = match layers.of_file(path) {
            Ok(place) => place,
            Err(problem) => {
                problems.push(problem);
                continue;
            }
        };
        if from.rank == layers.order.len() {
            continue;
        }
        let module = module_path(path);
        let named = paths(file, &module, &[]);
        let (own, _) = super::fake::imports(file, &module);
        let relative = named.relative.iter().filter_map(|segments| {
            let (first, rest) = segments.split_first()?;
            let (krate, full) = own.get(first)?.split_first()?;
            (krate == "crate").then(|| full.iter().chain(rest).cloned().collect::<Vec<_>>())
        });
        for abs in named.found.iter().cloned().chain(relative) {
            let abs = match abs.split_first() {
                Some((first, rest)) if roots.contains_key(first) => {
                    roots[first].iter().chain(rest).cloned().collect()
                }
                _ => abs,
            };
            let Some(to) = layers.of(&abs) else {
                continue;
            };
            // The entries use the plumbing of the join they live in.
            if from.module.starts_with(&entry) && Some(to.module.as_str()) == home {
                continue;
            }
            let edge = format!("{} -> {}", from.module, to.module);
            if to.rank > from.rank {
                edges
                    .entry(edge)
                    .or_insert_with(|| format!("{SOURCES}/{path}: {}", abs.join("::")));
            } else {
                down.insert(edge);
            }
        }
    }
    let listed: BTreeSet<&String> = loader.upward.iter().collect();
    for (edge, first) in edges.iter().filter(|(edge, _)| !listed.contains(edge)) {
        problems.push(format!(
            "{first}: {edge} goes up the loader's layers: move what it names down, or register it in a lower layer"
        ));
    }
    for edge in listed.iter().filter(|edge| !edges.contains_key(**edge)) {
        problems.push(listed_problem(
            &format!("the loader's {edge}"),
            down.contains(*edge),
            "[loader] upward",
        ));
    }
    Ok(problems)
}

/// Each name the loader's root re-exports, with the path it stands for.
fn root_exports(lib: &syn::File) -> BTreeMap<String, Vec<String>> {
    let mut roots = BTreeMap::new();
    for item in &lib.items {
        if let Item::Use(used) = item {
            for (name, full) in use_leaves(&used.tree) {
                if let Some(name) = name {
                    // `crate::` and `self::` at the root name the root itself.
                    let start =
                        usize::from(full.first().is_some_and(|s| s == "crate" || s == "self"));
                    roots.insert(name, full[start..].to_vec());
                }
            }
        }
    }
    roots
}

#[cfg(test)]
mod tests {
    use super::super::tests::Tree;
    use super::*;

    fn map(upward: &str) -> Map {
        let text = format!(
            "order = [\"base\", \"world\", \"ui\"]\njoins = []\nupward = []\n[modules]\n[loader]\nbelow = [\"engine\"]\njoins = [\"host\", \"start\"]\nentries = \"host/entries\"\nupward = [{upward}]\n"
        );
        toml::from_str(&text).unwrap_or_else(|e| panic!("{e}"))
    }

    fn tree(name: &str) -> Tree {
        let root = std::env::temp_dir().join(format!("xtask-loader-{name}-{}", std::process::id()));
        let _gone = std::fs::remove_dir_all(&root);
        let tree = Tree(root);
        tree.write(
            &format!("{SOURCES}/lib.rs"),
            "pub mod engine;\npub mod base;\npub mod world;\nmod host;\npub use world::map::Map;\npub use crate::world::map::open as open_map;",
        );
        tree.write(&format!("{SOURCES}/engine/log.rs"), "");
        tree.write(
            &format!("{SOURCES}/world/map.rs"),
            "use crate::engine::log;",
        );
        tree.write(&format!("{SOURCES}/host.rs"), "use crate::world::map;");
        tree
    }

    fn check(tree: &Tree, upward: &str) -> Vec<String> {
        problems(&tree.0, &map(upward)).unwrap_or_else(|e| vec![e])
    }

    #[test]
    fn edges_down_from_the_joins_and_from_entries_to_their_join_pass() {
        let tree = tree("down");
        tree.write(
            &format!("{SOURCES}/host/entries/base.rs"),
            "use crate::host::entry;\nuse crate::engine::log;",
        );
        assert_eq!(check(&tree, ""), Vec::<String>::new());
    }

    #[test]
    fn an_edge_up_by_any_path_fails_unless_listed() {
        let tree = tree("up");
        let cases = [
            ("use crate::world::map;", "base::net -> world::map"),
            (
                "use crate::world;\nfn f() { world::map::open(); }",
                "base::net -> world::map",
            ),
            ("fn f() { crate::Map::new(); }", "base::net -> world::map"),
            ("fn f() { crate::open_map(); }", "base::net -> world::map"),
            ("fn f() { crate::host::handle(); }", "base::net -> host"),
            (
                "fn f() { crate::host::entries::world::spawn(); }",
                "base::net -> entries::world",
            ),
        ];
        for (code, edge) in cases {
            tree.write(&format!("{SOURCES}/base/net.rs"), code);
            let problems = check(&tree, "");
            assert!(
                problems.len() == 1 && problems[0].contains(&format!("{edge} goes up")),
                "{code}: {problems:?}"
            );
            assert_eq!(check(&tree, &format!("\"{edge}\"")), Vec::<String>::new());
        }
    }

    #[test]
    fn a_listed_edge_gone_or_no_longer_up_and_a_stray_file_fail() {
        let tree = tree("stray");
        tree.write(&format!("{SOURCES}/sundry.rs"), "");
        let problems = check(
            &tree,
            "\"base::net -> world::map\", \"world::map -> engine::log\"",
        );
        assert!(
            problems
                .iter()
                .any(|p| p.contains("sundry.rs is in no layer")),
            "{problems:?}"
        );
        assert!(
            problems
                .iter()
                .any(|p| p.contains("base::net -> world::map is gone")),
            "{problems:?}"
        );
        assert!(
            problems
                .iter()
                .any(|p| p.contains("world::map -> engine::log does not go up")),
            "{problems:?}"
        );
        assert_eq!(problems.len(), 3, "{problems:?}");
    }
}
