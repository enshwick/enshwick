//! The domain rule of the API: each top-level module of `enshwick-core`
//! belongs to one domain of `crates/internal/core/domains.toml`, and a
//! module names only its own domain and those listed before it. The edges
//! up that order still in the code are listed there too; a new one, or a
//! listed one that is gone, fails, so the list only shrinks as domains are
//! peeled out. A domain peeled into a crate of its own depends on no
//! internal crate but those below every domain and the crates of the
//! domains before its own; the compiler holds the rest. The fake loader is
//! cut by the same domains, a file's domain named by its folder, and held
//! by the same rule with an `upward` list of its own.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use proc_macro2::{Ident, Spacing, TokenStream, TokenTree};
use syn::visit::{self, Visit};
use syn::{Item, ItemMod, UseTree, Visibility};

mod fake;

/// The map, from the workspace root.
const MAP: &str = "crates/internal/core/domains.toml";
/// Core's sources, from the workspace root.
const SOURCES: &str = "crates/internal/core/src";
/// The script namespaces' per-domain lists, under `src`.
const LISTS: &str = "script/namespaces/";

/// `domains.toml`.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Map {
    /// The domains, lowest first.
    order: Vec<String>,
    /// Files and folders, under `src`, that join every domain's list.
    joins: Vec<String>,
    /// Each domain's top-level modules still in core.
    modules: BTreeMap<String, Vec<String>>,
    /// The edges up the order still in the code, as `from -> to`.
    upward: Vec<String>,
    /// The internal crates below every domain.
    #[serde(default)]
    below: Vec<String>,
    /// Each peeled domain's crate, by its folder under `crates/internal`.
    #[serde(default)]
    crates: BTreeMap<String, String>,
    /// The fake loader, cut by the same domains.
    #[serde(default)]
    fake: fake::Fake,
}

/// The internal crates a peeled domain's crate depends on that are not
/// below it: neither one of `below` nor the crate of a domain before its
/// own.
fn crate_problems(root: &Path, map: &Map) -> Vec<String> {
    let manifest = |dir: &str| -> Result<toml::Table, String> {
        let path = format!("crates/internal/{dir}/Cargo.toml");
        let text = std::fs::read_to_string(root.join(&path)).map_err(|e| format!("{path}: {e}"))?;
        toml::from_str(&text).map_err(|e| format!("{path}: {e}"))
    };
    let mut problems = Vec::new();
    let mut lower: Vec<String> = map.below.clone();
    for domain in &map.order {
        let Some(dir) = map.crates.get(domain) else {
            continue;
        };
        let table = match manifest(dir) {
            Ok(table) => table,
            Err(error) => {
                problems.push(error);
                continue;
            }
        };
        let name = table
            .get("package")
            .and_then(|p| p.get("name"))
            .and_then(toml::Value::as_str)
            .unwrap_or(dir)
            .to_owned();
        let deps = dependencies(&table);
        for dep in deps
            .iter()
            .filter(|dep| dep.contains("enshwick") && !lower.contains(dep))
        {
            problems.push(format!(
                "crates/internal/{dir}/Cargo.toml: {name}, the crate of {domain}, depends on {dep}, which is not below it in {MAP}"
            ));
        }
        lower.push(name);
    }
    for domain in map.crates.keys().filter(|d| !map.order.contains(d)) {
        problems.push(format!(
            "{MAP}: the crate of {domain} is of no domain in the order"
        ));
    }
    problems
}

/// The packages a manifest's `[dependencies]` name, a renamed one by its
/// `package`, those of its `[target.*.dependencies]` included.
fn dependencies(manifest: &toml::Table) -> Vec<String> {
    let targets = manifest
        .get("target")
        .and_then(toml::Value::as_table)
        .into_iter()
        .flat_map(|targets| targets.values());
    std::iter::once(manifest.get("dependencies"))
        .chain(targets.map(|target| target.get("dependencies")))
        .flatten()
        .filter_map(toml::Value::as_table)
        .flat_map(|deps| {
            deps.iter().map(|(key, dep)| {
                dep.get("package")
                    .and_then(toml::Value::as_str)
                    .unwrap_or(key)
                    .to_owned()
            })
        })
        .collect()
}

/// The problems of the domain rule under `root`, one line each.
pub(crate) fn check(root: &Path) -> Vec<String> {
    match problems(root) {
        Ok(problems) => problems,
        Err(error) => vec![error],
    }
}

fn problems(root: &Path) -> Result<Vec<String>, String> {
    let text = std::fs::read_to_string(root.join(MAP))
        .map_err(|error| format!("reading {MAP}: {error}"))?;
    let map: Map = toml::from_str(&text).map_err(|error| format!("{MAP}: {error}"))?;
    let sources = root.join(SOURCES);
    let mut files = Vec::new();
    collect(&sources, &sources, &mut files)?;
    let mut problems = namespace_lists(&map, &files);
    problems.extend(crate_problems(root, &map));
    let joined = |path: &str| map.joins.iter().any(|join| path.starts_with(join.as_str()));
    let modules: BTreeSet<String> = files
        .iter()
        .filter(|(path, _)| !joined(path))
        .map(|(path, _)| top(path))
        .collect();
    let lib = files.iter().find(|(path, _)| path == "lib.rs");
    let mut roots = roots(lib.map_or("", |(_, text)| text))?;
    // A module's own name wins over an item the root re-exports under it.
    roots.retain(|name, _| !modules.contains(name));
    let mut edges = BTreeSet::new();
    for (path, text) in &files {
        let named = names(text, &module_path(path), &roots)
            .map_err(|e| format!("{SOURCES}/{path}: {e}"))?;
        if let Some(list) = path.strip_prefix(LISTS).and_then(|p| p.strip_suffix(".rs")) {
            problems.extend(list_strays(&map, list, &named));
        } else if !joined(path) {
            let from = top(path);
            for to in named
                .into_iter()
                .filter(|to| *to != from && modules.contains(to))
            {
                edges.insert((from.clone(), to));
            }
        }
    }
    problems.extend(judge(&map, &modules, &edges));
    problems.extend(fake::problems(root, &map, &roots)?);
    Ok(problems)
}

/// What the namespace list of `list` names from another domain: each list
/// holds its own domain's namespaces, and `Load`, which carries every
/// domain's starting functions.
fn list_strays(map: &Map, list: &str, named: &BTreeSet<String>) -> Vec<String> {
    let mut problems = Vec::new();
    let own = top(LISTS);
    for (domain, modules) in map.modules.iter().filter(|(domain, _)| *domain != list) {
        for module in modules
            .iter()
            .filter(|m| named.contains(*m) && *m != "load" && **m != own)
        {
            problems.push(format!(
                "{SOURCES}/{LISTS}{list}.rs names {module} of {domain}: move its namespace to {LISTS}{domain}.rs"
            ));
        }
    }
    problems
}

/// The script namespaces' join: one list file per domain under
/// `script/namespaces/`, named after it, joined in the map's order.
fn namespace_lists(map: &Map, files: &[(String, String)]) -> Vec<String> {
    let mut problems = Vec::new();
    let listed: Vec<&str> = files
        .iter()
        .filter_map(|(path, _)| path.strip_prefix(LISTS)?.strip_suffix(".rs"))
        .collect();
    for domain in listed.iter().filter(|d| !map.order.iter().any(|o| o == *d)) {
        problems.push(format!(
            "{SOURCES}/{LISTS}{domain}.rs names no domain of {MAP}"
        ));
    }
    let expected: Vec<&str> = map
        .order
        .iter()
        .map(String::as_str)
        .filter(|domain| listed.contains(domain))
        .collect();
    let joined: Vec<&str> = files
        .iter()
        .find(|(path, _)| path == "script/namespaces.rs")
        .map(|(_, text)| {
            text.lines()
                .filter_map(|line| line.trim().strip_suffix("::NAMESPACES,"))
                .collect()
        })
        .unwrap_or_default();
    if joined != expected {
        problems.push(format!(
            "{SOURCES}/script/namespaces.rs joins {joined:?}, not the lists in the order of {MAP}: {expected:?}"
        ));
    }
    problems
}

/// What breaks the rule, given the map, core's top-level modules and the
/// edges between them.
fn judge(map: &Map, modules: &BTreeSet<String>, edges: &BTreeSet<(String, String)>) -> Vec<String> {
    let mut problems = Vec::new();
    let mut domain_of = BTreeMap::new();
    for (domain, listed) in &map.modules {
        if !map.order.contains(domain) {
            problems.push(format!("{MAP}: domain {domain} is not in the order"));
        }
        for module in listed {
            if let Some(other) = domain_of.insert(module.as_str(), domain.as_str()) {
                problems.push(format!("{MAP}: {module} is in {other} and {domain}"));
            }
        }
    }
    let rank = |module: &str| {
        domain_of
            .get(module)
            .and_then(|domain| map.order.iter().position(|d| d == domain))
    };
    for module in modules
        .iter()
        .filter(|m| !domain_of.contains_key(m.as_str()))
    {
        problems.push(format!("{MAP}: the module {module} has no domain"));
    }
    for module in domain_of.keys().filter(|m| !modules.contains(**m)) {
        problems.push(format!("{MAP}: {module} is no module of core"));
    }
    let mut upward = BTreeSet::new();
    for (from, to) in edges {
        if let (Some(a), Some(b)) = (rank(from), rank(to))
            && b > a
        {
            upward.insert(format!("{from} -> {to}"));
        }
    }
    let listed: BTreeSet<String> = map.upward.iter().cloned().collect();
    for edge in upward.difference(&listed) {
        problems.push(format!(
            "{edge} goes up the domain order: move what it names down into base or a lower domain"
        ));
    }
    for edge in listed.difference(&upward) {
        problems.push(format!(
            "{MAP}: {edge} is gone from the code; drop it from upward"
        ));
    }
    problems
}

/// Every `.rs` file under `dir`, but tests, as its path under `root` with
/// `/` and its text.
fn collect(root: &Path, dir: &Path, out: &mut Vec<(String, String)>) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for entry in entries {
        let path = entry.map_err(|e| format!("{}: {e}", dir.display()))?.path();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        if path.is_dir() {
            if name != "tests" {
                collect(root, &path, out)?;
            }
        } else if path.extension().is_some_and(|e| e == "rs") && name != "tests.rs" {
            let text =
                std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            let relative = path.strip_prefix(root).unwrap_or(&path);
            let shown = relative
                .components()
                .map(|c| c.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            out.push((shown, text));
        }
    }
    Ok(())
}

/// The top-level module a file under `src` belongs to.
fn top(path: &str) -> String {
    let first = path.split('/').next().unwrap_or(path);
    first.strip_suffix(".rs").unwrap_or(first).to_owned()
}

/// The module path of a file under `src`: none for `lib.rs`, `ui` for
/// `ui.rs` and `ui/mod.rs`, `ui::frame` for `ui/frame.rs`.
fn module_path(path: &str) -> Vec<String> {
    let path = path.strip_suffix(".rs").unwrap_or(path);
    let path = path.strip_suffix("/mod").unwrap_or(path);
    if path == "lib" {
        return Vec::new();
    }
    path.split('/').map(str::to_owned).collect()
}

/// The items `lib.rs` re-exports at the crate root, each with the module it
/// comes from: `Load` with `load` for `pub use load::{Load, Mod, Off}`.
fn roots(lib: &str) -> Result<BTreeMap<String, String>, String> {
    fn leaves(tree: &UseTree, module: Option<&str>, out: &mut BTreeMap<String, String>) {
        match tree {
            UseTree::Path(path) => {
                let name = path.ident.to_string();
                leaves(&path.tree, Some(module.unwrap_or(&name)), out);
            }
            UseTree::Name(name) => {
                if let Some(module) = module {
                    out.insert(name.ident.to_string(), module.to_owned());
                }
            }
            UseTree::Rename(rename) => {
                if let Some(module) = module {
                    out.insert(rename.rename.to_string(), module.to_owned());
                }
            }
            UseTree::Group(group) => {
                for tree in &group.items {
                    leaves(tree, module, out);
                }
            }
            UseTree::Glob(_) => {}
        }
    }
    let file = syn::parse_file(lib).map_err(|e| format!("{SOURCES}/lib.rs: {e}"))?;
    let mut roots = BTreeMap::new();
    for item in &file.items {
        if let Item::Use(item) = item
            && matches!(item.vis, Visibility::Public(_))
        {
            leaves(&item.tree, None, &mut roots);
        }
    }
    Ok(roots)
}

/// The top-level modules `text`, a file of module `module`, names: the
/// first segment under the crate root of each `crate::`, `self::` and
/// `super::` path, a root re-export read through `roots`; comments and
/// `#[cfg(test)]` items left out, macro bodies read as tokens.
fn names(
    text: &str,
    module: &[String],
    roots: &BTreeMap<String, String>,
) -> Result<BTreeSet<String>, String> {
    let (found, _) = paths(text, module, &[])?;
    Ok(found
        .into_iter()
        .filter_map(|abs| {
            let first = abs.into_iter().next()?;
            Some(roots.get(&first).cloned().unwrap_or(first))
        })
        .collect())
}

/// Paths of a file: those under its crate's root, absolute, and those that
/// start at an external crate.
type Named = (BTreeSet<Vec<String>>, BTreeSet<Vec<String>>);

/// The paths `text`, a file of module `module`, names: those under its
/// crate's root, absolute, and those that start at one of `externs`;
/// comments and `#[cfg(test)]` items left out, macro bodies read as tokens.
fn paths(text: &str, module: &[String], externs: &[&str]) -> Result<Named, String> {
    let file = syn::parse_file(text).map_err(|e| e.to_string())?;
    let mut walk = Walk::new(module, externs);
    walk.visit_file(&file);
    Ok((walk.found, walk.external))
}

/// The walk of [`paths`] over one file.
struct Walk<'a> {
    /// The module the walk stands in, inline modules counted.
    module: Vec<String>,
    /// The crates whose paths count as external.
    externs: &'a [&'a str],
    /// The absolute paths under the crate root.
    found: BTreeSet<Vec<String>>,
    /// The paths that start at one of `externs`.
    external: BTreeSet<Vec<String>>,
}

impl<'a> Walk<'a> {
    fn new(module: &[String], externs: &'a [&'a str]) -> Self {
        Self {
            module: module.to_vec(),
            externs,
            found: BTreeSet::new(),
            external: BTreeSet::new(),
        }
    }

    /// The absolute path `segments` stands for when it starts at `crate`,
    /// `self` or a chain of `super` inside the crate.
    fn resolve(&self, segments: &[String]) -> Option<Vec<String>> {
        let supers = segments.iter().take_while(|s| *s == "super").count();
        let (base, rest) = match segments.first().map(String::as_str) {
            Some("crate") => (&[][..], &segments[1..]),
            Some("self") => (&self.module[..], &segments[1..]),
            Some("super") if supers <= self.module.len() => (
                &self.module[..self.module.len() - supers],
                &segments[supers..],
            ),
            _ => return None,
        };
        Some(base.iter().chain(rest).cloned().collect())
    }

    /// Records the path of `segments`, when it starts at the crate root, in
    /// the module or at an external crate.
    fn path(&mut self, segments: &[String]) {
        if let Some(absolute) = self.resolve(segments) {
            self.found.insert(absolute);
        } else if segments
            .first()
            .is_some_and(|first| self.externs.contains(&first.as_str()))
        {
            self.external.insert(segments.to_vec());
        }
    }

    /// Records each path of a `use` tree under `prefix`.
    fn tree(&mut self, prefix: &mut Vec<String>, tree: &UseTree) {
        match tree {
            UseTree::Path(path) => {
                prefix.push(path.ident.to_string());
                self.tree(prefix, &path.tree);
                prefix.pop();
            }
            UseTree::Name(name) => {
                prefix.push(name.ident.to_string());
                self.path(prefix);
                prefix.pop();
            }
            UseTree::Rename(rename) => {
                prefix.push(rename.ident.to_string());
                self.path(prefix);
                prefix.pop();
            }
            UseTree::Group(group) => {
                for tree in &group.items {
                    self.tree(prefix, tree);
                }
            }
            UseTree::Glob(_) => self.path(prefix),
        }
    }

    /// Records each `a::b` path in a macro body, `$crate::` included.
    fn tokens(&mut self, tokens: TokenStream) {
        let trees: Vec<TokenTree> = tokens.into_iter().collect();
        let mut at = 0;
        while at < trees.len() {
            match &trees[at] {
                TokenTree::Group(group) => {
                    self.tokens(group.stream());
                    at += 1;
                }
                TokenTree::Ident(first) => {
                    let mut segments = vec![first.to_string()];
                    at += 1;
                    while let [
                        TokenTree::Punct(a),
                        TokenTree::Punct(b),
                        TokenTree::Ident(next),
                        ..,
                    ] = &trees[at..]
                        && a.as_char() == ':'
                        && a.spacing() == Spacing::Joint
                        && b.as_char() == ':'
                    {
                        segments.push(next.to_string());
                        at += 3;
                    }
                    self.path(&segments);
                }
                _ => at += 1,
            }
        }
    }
}

impl<'ast> Visit<'ast> for Walk<'_> {
    fn visit_item(&mut self, item: &'ast Item) {
        if !test_only(item) {
            visit::visit_item(self, item);
        }
    }

    fn visit_item_mod(&mut self, item: &'ast ItemMod) {
        let inline = item.content.is_some();
        if inline {
            self.module.push(item.ident.to_string());
        }
        visit::visit_item_mod(self, item);
        if inline {
            self.module.pop();
        }
    }

    fn visit_path(&mut self, path: &'ast syn::Path) {
        let segments: Vec<String> = path.segments.iter().map(|s| s.ident.to_string()).collect();
        if path.leading_colon.is_none() {
            self.path(&segments);
        }
        visit::visit_path(self, path);
    }

    fn visit_use_tree(&mut self, tree: &'ast UseTree) {
        self.tree(&mut Vec::new(), tree);
    }

    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        self.tokens(mac.tokens.clone());
        visit::visit_macro(self, mac);
    }
}

/// Whether `item` exists only in tests, under a plain `#[cfg(test)]`.
fn test_only(item: &Item) -> bool {
    let attrs = match item {
        Item::Const(item) => &item.attrs,
        Item::Enum(item) => &item.attrs,
        Item::Fn(item) => &item.attrs,
        Item::Impl(item) => &item.attrs,
        Item::Macro(item) => &item.attrs,
        Item::Mod(item) => &item.attrs,
        Item::Static(item) => &item.attrs,
        Item::Struct(item) => &item.attrs,
        Item::Trait(item) => &item.attrs,
        Item::Type(item) => &item.attrs,
        Item::Use(item) => &item.attrs,
        _ => return false,
    };
    attrs.iter().any(|attr| {
        attr.path().is_ident("cfg") && attr.parse_args::<Ident>().is_ok_and(|arg| arg == "test")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(upward: &[&str]) -> Map {
        Map {
            order: vec!["base".into(), "world".into(), "ui".into()],
            joins: Vec::new(),
            modules: [
                ("base".to_owned(), vec!["host".to_owned()]),
                ("world".to_owned(), vec!["map".to_owned()]),
                ("ui".to_owned(), vec!["ui".to_owned()]),
            ]
            .into(),
            upward: upward.iter().map(|&s| s.to_owned()).collect(),
            below: Vec::new(),
            crates: BTreeMap::new(),
            fake: fake::Fake::default(),
        }
    }

    #[test]
    fn a_peeled_crate_may_name_only_the_crates_below_it() {
        let root = std::env::temp_dir().join(format!("xtask-crates-{}", std::process::id()));
        let manifest = |dir: &str, name: &str, deps: &str| {
            let path = root.join("crates/internal").join(dir);
            std::fs::create_dir_all(&path).unwrap();
            std::fs::write(
                path.join("Cargo.toml"),
                format!("[package]\nname = \"{name}\"\n[dependencies]\n{deps}"),
            )
            .unwrap();
        };
        manifest(
            "base",
            "enshwick-base",
            "enshwick-abi.workspace = true\nenshwick-ui.workspace = true\nserde = \"1\"\n",
        );
        manifest(
            "ui",
            "enshwick-ui",
            "enshwick-base.workspace = true\nenshwick-core.workspace = true\n",
        );
        let mut map = map(&[]);
        map.below = vec!["enshwick-abi".to_owned()];
        map.crates = [
            ("base".to_owned(), "base".to_owned()),
            ("ui".to_owned(), "ui".to_owned()),
            ("sound".to_owned(), "sound".to_owned()),
        ]
        .into();
        let problems = crate_problems(&root, &map);
        let _gone = std::fs::remove_dir_all(&root);
        assert_eq!(problems.len(), 3, "{problems:?}");
        assert!(problems[0].contains("enshwick-base, the crate of base, depends on enshwick-ui"));
        assert!(problems[1].contains("enshwick-ui, the crate of ui, depends on enshwick-core"));
        assert!(problems[2].contains("the crate of sound is of no domain"));
    }

    #[test]
    fn a_renamed_or_target_dependency_counts_by_its_package() -> Result<(), toml::de::Error> {
        let manifest: toml::Table = toml::from_str(
            "[dependencies]\nserde = \"1\"\nup = { package = \"enshwick-core\", workspace = true }\n\n[target.'cfg(windows)'.dependencies]\nenshwick-ui.workspace = true\n",
        )?;
        assert_eq!(
            dependencies(&manifest),
            ["serde", "enshwick-core", "enshwick-ui"]
        );
        Ok(())
    }

    fn set(items: &[&str]) -> BTreeSet<String> {
        items.iter().map(|&s| s.to_owned()).collect()
    }

    fn edges(pairs: &[(&str, &str)]) -> BTreeSet<(String, String)> {
        pairs
            .iter()
            .map(|&(a, b)| (a.to_owned(), b.to_owned()))
            .collect()
    }

    #[test]
    fn names_reads_paths_groups_nested_groups_and_macro_bodies() -> Result<(), String> {
        let code = "use crate::map::Map;\nuse crate::{host::Host, ui::{Ui, frame}};\nfn f() { crate::sound::play(); let a = foo_crate::x; log!(crate::time::now()); }\nmacro_rules! m { () => { $crate::world::W } }";
        assert_eq!(
            names(code, &module("x"), &BTreeMap::new())?,
            set(&["host", "map", "sound", "time", "ui", "world"])
        );
        Ok(())
    }

    fn module(path: &str) -> Vec<String> {
        path.split("::").map(str::to_owned).collect()
    }

    #[test]
    fn names_resolves_super_and_self_chains_and_counts_inline_modules() -> Result<(), String> {
        let code = "use super::super::ui::Ui;\nuse super::frame::F;\nuse self::part::P;\nuse super::super::{host::H, map};\nmod inner { use super::super::super::sound::S; use super::super::time::T; }";
        assert_eq!(
            names(code, &module("world::props"), &BTreeMap::new())?,
            set(&["host", "map", "sound", "ui", "world"])
        );
        Ok(())
    }

    #[test]
    fn paths_keeps_a_named_crate_s_paths_apart_and_drops_other_crates() -> Result<(), String> {
        let code = "use enshwick_core::ui::Frame;\nuse std::fmt;\nfn f() { other::x(); enshwick_core::chat::send(); }";
        let (found, external) = paths(code, &module("test::world"), &[fake::CORE_CRATE])?;
        assert!(found.is_empty(), "{found:?}");
        let external: Vec<String> = external.iter().map(|p| p.join("::")).collect();
        assert_eq!(
            external,
            ["enshwick_core::chat::send", "enshwick_core::ui::Frame"]
        );
        Ok(())
    }

    #[test]
    fn names_skips_comments_strings_and_test_items_but_reads_code_after_them() -> Result<(), String>
    {
        let code = "/// crate::ui::Ui\nuse crate::map; // crate::host\nconst U: &str = \"https://x crate::world\"; use crate::time::T;\n#[cfg(test)]\nmod tests { use crate::ui; }\nuse crate::sound::S;";
        assert_eq!(
            names(code, &module("x"), &BTreeMap::new())?,
            set(&["map", "sound", "time"])
        );
        Ok(())
    }

    #[test]
    fn names_reads_a_root_re_export_as_the_module_it_comes_from() -> Result<(), String> {
        let roots = roots(
            "pub use world_error::WorldError;\npub use load::{Load as L, Mod};\nuse ui::Hidden;",
        )?;
        assert_eq!(
            roots.get("WorldError").map(String::as_str),
            Some("world_error")
        );
        assert_eq!(roots.get("L").map(String::as_str), Some("load"));
        assert!(!roots.contains_key("Hidden"));
        let code = "use crate::WorldError;\nfn f() { crate::L::x(); }";
        assert_eq!(
            names(code, &module("x"), &roots)?,
            set(&["load", "world_error"])
        );
        Ok(())
    }

    #[test]
    fn a_namespace_list_naming_another_domain_fails_but_load_passes() {
        let problems = list_strays(&map(&[]), "world", &set(&["map", "load", "ui"]));
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].contains("names ui of ui"), "{problems:?}");
    }

    #[test]
    fn module_path_names_the_modules_of_a_file_path() {
        assert_eq!(module_path("lib.rs"), Vec::<String>::new());
        assert_eq!(module_path("ui.rs"), module("ui"));
        assert_eq!(module_path("ui/mod.rs"), module("ui"));
        assert_eq!(module_path("ui/frame.rs"), module("ui::frame"));
        assert_eq!(module_path("ui/frame/mod.rs"), module("ui::frame"));
    }

    #[test]
    fn an_edge_down_the_order_passes_and_one_up_fails_unless_listed() {
        let modules = set(&["host", "map", "ui"]);
        let down = edges(&[("ui", "map"), ("map", "host")]);
        assert!(judge(&map(&[]), &modules, &down).is_empty());
        let up = edges(&[("map", "ui")]);
        let problems = judge(&map(&[]), &modules, &up);
        assert!(problems[0].starts_with("map -> ui goes up"), "{problems:?}");
        assert!(judge(&map(&["map -> ui"]), &modules, &up).is_empty());
    }

    #[test]
    fn a_listed_edge_that_is_gone_and_a_module_without_domain_fail() {
        let modules = set(&["host", "map", "ui", "sound"]);
        let problems = judge(&map(&["map -> ui"]), &modules, &BTreeSet::new());
        assert!(problems.iter().any(|p| p.contains("sound has no domain")));
        assert!(problems.iter().any(|p| p.contains("map -> ui is gone")));
    }

    #[test]
    fn a_tree_with_an_edge_up_a_stray_namespace_and_a_join_out_of_order_fails() {
        /// Removes the tree however the test ends.
        struct Tree(std::path::PathBuf);
        impl Drop for Tree {
            fn drop(&mut self) {
                let _gone = std::fs::remove_dir_all(&self.0);
            }
        }
        let root = std::env::temp_dir().join(format!("xtask-domains-{}", std::process::id()));
        let _gone = std::fs::remove_dir_all(&root);
        let tree = Tree(root.clone());
        let src = tree.0.join(SOURCES);
        let write = |path: &str, text: &str| {
            let path = src.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        };
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(
            root.join(MAP),
            "order = [\"base\", \"ui\"]\njoins = [\"lib.rs\", \"script/namespaces\"]\nupward = []\n[modules]\nbase = [\"host\", \"script\"]\nui = [\"ui\"]\n",
        )
        .unwrap();
        write("lib.rs", "mod host; mod ui; mod script;\npub use ui::Ui;");
        write("host.rs", "use crate::Ui;\nuse crate::script;");
        write("ui.rs", "use crate::host::Host;");
        write("script.rs", "mod namespaces;");
        write(
            "script/namespaces.rs",
            "const D: &[&[N]] = &[\n    ui::NAMESPACES,\n    base::NAMESPACES,\n];",
        );
        write(
            "script/namespaces/base.rs",
            "const N: &[u8] = &[crate::ui::X];",
        );
        write("script/namespaces/ui.rs", "");
        let fake = tree.0.join(fake::SOURCES);
        std::fs::create_dir_all(fake.join(fake::ROOT)).unwrap();
        std::fs::write(fake.join(format!("{}.rs", fake::ROOT)), "").unwrap();
        let problems = check(&root);
        assert!(
            problems.iter().any(|p| p.starts_with("host -> ui goes up")),
            "{problems:?}"
        );
        assert!(
            problems
                .iter()
                .any(|p| p.contains("joins [\"ui\", \"base\"]")),
            "{problems:?}"
        );
        assert!(
            problems
                .iter()
                .any(|p| p.contains("base.rs names ui of ui")),
            "{problems:?}"
        );
        assert_eq!(problems.len(), 3, "{problems:?}");
    }

    #[test]
    fn the_map_of_core_holds_for_its_code() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        assert_eq!(check(&root), Vec::<String>::new());
    }
}
