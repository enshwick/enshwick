//! The domain rule in the loader: each file under `src/<layer>/` belongs to
//! that layer, the layers below the domains first and then the domains in
//! their order, and each file under `src/host/entries/<domain>` to that
//! domain; the joins, `lib.rs` and the modules `[loader] joins` names, sit
//! above them all. A layer's modules name only their own layer and those
//! before it, and no join, by any path, glob or exported macro; methods a
//! module adds to another's type for others to call count as an edge from
//! that type's module. The edges up still in the code are listed, module by
//! module, and may only go.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use syn::Item;

use super::walk::{ident, imports};
use super::{
    MAP, Map, collect, drop_test_modules, listed_problem, module_path, paths, test_only, use_leaves,
};

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
    /// The folder of [`Loader::entries`], by module.
    entries: Vec<&'a str>,
    joins: &'a [String],
}

impl Layers<'_> {
    /// The place of the absolute path `abs`, `None` for one outside the
    /// layers and joins or one that names a layer alone.
    fn of(&self, abs: &[String]) -> Option<Place> {
        let first = abs.first()?.as_str();
        let entries = &self.entries;
        if abs.len() > entries.len() && abs.iter().zip(entries).all(|(a, b)| a == b) {
            let domain = abs[entries.len()].as_str();
            // An entries file named after no domain belongs to its join.
            if let Some(rank) = self.order.iter().position(|l| *l == domain) {
                return Some(Place {
                    rank,
                    module: format!("{}::{domain}", entries.last().unwrap_or(&"")),
                });
            }
        }
        if self.joins.iter().any(|join| join == first) {
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

    /// The place of `abs` as [`Self::of`] finds it, or of the layer it names
    /// alone: a glob that takes a layer whole, a layer's own file.
    fn of_or_layer(&self, abs: &[String]) -> Option<Place> {
        self.of(abs).or_else(|| {
            let [layer] = abs else {
                return None;
            };
            let rank = self.order.iter().position(|l| l == layer)?;
            Some(Place {
                rank,
                module: layer.clone(),
            })
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
        self.of_or_layer(&module).ok_or_else(|| {
            format!("{SOURCES}/{path} is in no layer of {MAP}: move it into a layer's folder")
        })
    }
}

pub(super) fn problems(root: &Path, map: &Map) -> Result<Vec<String>, String> {
    let loader = &map.loader;
    let sources = root.join(SOURCES);
    if !sources.is_dir() {
        let held = !(loader.below.is_empty() && loader.upward.is_empty());
        return Ok(held
            .then(|| {
                format!(
                    "{SOURCES} is missing, so the [loader] table of {MAP} holds nothing: fix the path or drop the table"
                )
            })
            .into_iter()
            .collect());
    }
    let layers = Layers {
        order: loader
            .below
            .iter()
            .chain(&map.order)
            .map(String::as_str)
            .collect(),
        entries: loader.entries.split('/').collect(),
        joins: &loader.joins,
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
    let (roots, types) = roots_and_types(&parsed, &layers);
    let home = layers.entries.first().copied();
    let entry = format!("{}::", layers.entries.last().copied().unwrap_or_default());
    let mut edges = Edges::default();
    for (path, file) in &parsed {
        let (path, file) = (*path, file);
        let from = match layers.of_file(path) {
            Ok(place) => place,
            Err(problem) => {
                problems.push(problem);
                continue;
            }
        };
        // Methods a file adds to a type of another module: whoever holds the
        // type may call them, so the type's module depends on the file.
        // shortcut: a type name defined in two files is skipped; rename one
        // when such an impl needs holding.
        for name in inherent_impls(file) {
            if let Some([to]) = types.get(&name).map(Vec::as_slice) {
                edges.add(to, &from, || format!("{SOURCES}/{path}: impl {name}"));
            }
        }
        if from.rank == layers.order.len() {
            continue;
        }
        for (abs, to) in named(file, &module_path(path), &roots, &layers) {
            // The entries use the plumbing of the join they live in.
            if from.module.starts_with(&entry) && Some(to.module.as_str()) == home {
                continue;
            }
            edges.add(&from, &to, || {
                format!("{SOURCES}/{path}: {}", abs.join("::"))
            });
        }
    }
    let listed: BTreeSet<&String> = loader.upward.iter().collect();
    for (edge, first) in edges.up.iter().filter(|(edge, _)| !listed.contains(edge)) {
        problems.push(format!(
            "{first}: {edge} goes up the loader's layers: move what it names down, or register it in a lower layer"
        ));
    }
    for edge in listed.iter().filter(|edge| !edges.up.contains_key(**edge)) {
        problems.push(listed_problem(
            &format!("the loader's {edge}"),
            edges.down.contains(*edge),
            "[loader] upward",
        ));
    }
    Ok(problems)
}

/// The edges the check finds: those up the layers, each with where it was
/// first seen, and the rest.
#[derive(Default)]
struct Edges {
    up: BTreeMap<String, String>,
    down: BTreeSet<String>,
}

impl Edges {
    /// Adds the edge from `from` to `to`, seen `at`.
    fn add(&mut self, from: &Place, to: &Place, at: impl FnOnce() -> String) {
        let edge = format!("{} -> {}", from.module, to.module);
        if to.rank > from.rank {
            self.up.entry(edge).or_insert_with(at);
        } else {
            self.down.insert(edge);
        }
    }
}

/// Each name the root stands for, `lib.rs`'s re-exports and the exported
/// macros, which sit at the root but belong to their file's module; and the
/// places of the types each file defines, by name.
fn roots_and_types(
    parsed: &[(&str, syn::File)],
    layers: &Layers<'_>,
) -> (BTreeMap<String, Vec<String>>, BTreeMap<String, Vec<Place>>) {
    let mut roots = parsed
        .iter()
        .find(|(path, _)| *path == "lib.rs")
        .map(|(_, lib)| root_exports(lib))
        .unwrap_or_default();
    let mut types: BTreeMap<String, Vec<Place>> = BTreeMap::new();
    for (path, file) in parsed {
        for name in exported_macros(file) {
            roots.insert(name, module_path(path));
        }
        if let Ok(place) = layers.of_file(path) {
            for name in type_names(file) {
                types.entry(name).or_default().push(place.clone());
            }
        }
    }
    (roots, types)
}

/// The places `file`, of module `module`, names by a path, an import or a
/// glob, each with the path that names it.
fn named(
    file: &syn::File,
    module: &[String],
    roots: &BTreeMap<String, Vec<String>>,
    layers: &Layers<'_>,
) -> Vec<(Vec<String>, Place)> {
    let named = paths(file, module, &[]);
    let (own, globs) = imports(file, module);
    let relative = named.relative.iter().filter_map(|segments| {
        let (first, rest) = segments.split_first()?;
        let (krate, full) = own.get(first)?.split_first()?;
        (krate == "crate").then(|| full.iter().chain(rest).cloned().collect::<Vec<_>>())
    });
    let whole = globs.iter().filter_map(|glob| {
        let (krate, full) = glob.split_first()?;
        (krate == "crate").then(|| full.to_vec())
    });
    let paths = named
        .found
        .iter()
        .cloned()
        .chain(relative)
        .map(|abs| (abs, false));
    paths
        .chain(whole.map(|abs| (abs, true)))
        .filter_map(|(abs, glob)| {
            let abs: Vec<String> = match abs.split_first() {
                Some((first, rest)) if roots.contains_key(first) => {
                    roots[first].iter().chain(rest).cloned().collect()
                }
                _ => abs,
            };
            let to = if glob {
                layers.of_or_layer(&abs)
            } else {
                layers.of(&abs)
            }?;
            Some((abs, to))
        })
        .collect()
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

/// The macros `file` exports at the crate's root, tests left out.
fn exported_macros(file: &syn::File) -> impl Iterator<Item = String> + '_ {
    file.items.iter().filter_map(|item| match item {
        Item::Macro(mac) if mac.attrs.iter().any(|a| a.path().is_ident("macro_export")) => {
            mac.ident.as_ref().map(ToString::to_string)
        }
        _ => None,
    })
}

/// The types `file` defines at its top, tests left out.
fn type_names(file: &syn::File) -> impl Iterator<Item = String> + '_ {
    file.items
        .iter()
        .filter(|item| matches!(item, Item::Struct(_) | Item::Enum(_) | Item::Union(_)))
        .filter(|item| !test_only(item))
        .filter_map(ident)
}

/// The types `file` adds inherent methods to at its top that code outside
/// its module may call, by their last name, tests left out.
fn inherent_impls(file: &syn::File) -> impl Iterator<Item = String> + '_ {
    let shown = |imp: &syn::ItemImpl| {
        imp.items.iter().any(|item| {
            matches!(item, syn::ImplItem::Fn(f) if !matches!(f.vis, syn::Visibility::Inherited))
        })
    };
    file.items
        .iter()
        .filter(|item| !test_only(item))
        .filter_map(move |item| match item {
            Item::Impl(imp) if imp.trait_.is_none() && shown(imp) => match &*imp.self_ty {
                syn::Type::Path(ty) => ty.path.segments.last().map(|s| s.ident.to_string()),
                _ => None,
            },
            _ => None,
        })
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
            "use crate::engine::log;\n#[macro_export]\nmacro_rules! paint { () => {} }",
        );
        tree.write(&format!("{SOURCES}/host.rs"), "use crate::world::map;");
        tree.write(
            &format!("{SOURCES}/host/entries/shared.rs"),
            "use crate::engine::log;",
        );
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
            (
                "fn f() { use crate::world; world::map::open(); }",
                "base::net -> world::map",
            ),
            (
                "use crate::world::map::*;\nfn f() { open(); }",
                "base::net -> world::map",
            ),
            (
                "use crate::world::*;\nfn f() { map::open(); }",
                "base::net -> world",
            ),
            ("fn f() { crate::paint!(); }", "base::net -> world::map"),
            (
                "fn f() { crate::host::entries::shared::x(); }",
                "base::net -> host",
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
    fn methods_a_higher_layer_adds_to_a_lower_layer_s_type_go_up() {
        let tree = tree("impl");
        tree.write(&format!("{SOURCES}/engine/loader.rs"), "pub struct Loader;");
        tree.write(
            &format!("{SOURCES}/world/weather.rs"),
            "impl crate::engine::loader::Loader { pub(crate) fn step(&self) {} }\nimpl Clone for crate::engine::loader::Loader { fn clone(&self) -> Self { Self } }",
        );
        tree.write(
            &format!("{SOURCES}/world/map.rs"),
            "impl crate::engine::loader::Loader { fn own(&self) {} }",
        );
        let problems = check(&tree, "");
        assert!(
            problems.len() == 1 && problems[0].contains("engine::loader -> world::weather goes up"),
            "{problems:?}"
        );
        assert_eq!(
            check(&tree, "\"engine::loader -> world::weather\""),
            Vec::<String>::new()
        );
    }

    #[test]
    fn a_missing_loader_with_a_table_fails_and_without_one_passes() {
        let root = std::env::temp_dir().join(format!("xtask-loader-none-{}", std::process::id()));
        let _gone = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap_or_else(|e| panic!("{e}"));
        let found = problems(&root, &map("\"base::net -> world::map\"")).unwrap_or_default();
        assert!(
            found.len() == 1 && found[0].contains("is missing"),
            "{found:?}"
        );
        let without: Map = toml::from_str("order = []\njoins = []\nupward = []\n[modules]\n")
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            problems(&root, &without).unwrap_or_default(),
            Vec::<String>::new()
        );
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
