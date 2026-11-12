//! The domain rule of the API: each top-level module of `enshwick-core`
//! belongs to one domain of `crates/internal/core/domains.toml`, and a
//! module names only its own domain and those listed before it, by a path
//! or by a call on `self` in its `Load` methods to one another module adds to
//! `Load`. The edges
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
use syn::{Attribute, ImplItem, Item, ItemMod, MetaNameValue, TraitItem, UseTree, Visibility};

mod calls;
mod deps;
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
    let mut problems = Vec::new();
    let mut lower: Vec<String> = map.below.clone();
    for domain in &map.order {
        let Some(dir) = map.crates.get(domain) else {
            continue;
        };
        let (table, name) = match package(root, dir) {
            Ok(package) => package,
            Err(error) => {
                problems.push(error);
                continue;
            }
        };
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

/// The manifest of the internal crate in `crates/internal/<dir>` and the
/// name of its package.
fn package(root: &Path, dir: &str) -> Result<(toml::Table, String), String> {
    let path = format!("crates/internal/{dir}/Cargo.toml");
    let text = std::fs::read_to_string(root.join(&path)).map_err(|e| format!("{path}: {e}"))?;
    let table: toml::Table = toml::from_str(&text).map_err(|e| format!("{path}: {e}"))?;
    let name = table
        .get("package")
        .and_then(|p| p.get("name"))
        .and_then(toml::Value::as_str)
        .unwrap_or(dir)
        .to_owned();
    Ok((table, name))
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
    drop_test_modules(&mut files);
    files.sort();
    let mut problems = namespace_lists(&map, &files);
    problems.extend(crate_problems(root, &map));
    problems.extend(deps::problems(root, &map)?);
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
    let lists = module_path(LISTS.trim_end_matches('/'));
    let mut edges = BTreeSet::new();
    // Each method a module adds to `Load`, and the methods each module's
    // `Load` methods call on `self`.
    let mut owner = BTreeMap::new();
    let mut calls = Vec::new();
    for (path, text) in &files {
        let file = syn::parse_file(text).map_err(|e| format!("{SOURCES}/{path}: {e}"))?;
        if !joined(path) {
            let (defined, called) = calls::load_methods(&file);
            owner.extend(defined.into_iter().map(|name| (name, top(path))));
            calls.push((top(path), called));
        }
        let found = paths(&file, &module_path(path), &[]).found;
        if let Some(list) = path.strip_prefix(LISTS).and_then(|p| p.strip_suffix(".rs")) {
            // A list's paths into its own folder, `self::` ones among them,
            // stay inside the lists.
            let named = tops(
                found.into_iter().filter(|abs| !abs.starts_with(&lists)),
                &roots,
            );
            problems.extend(list_strays(&map, list, &named));
        } else if !joined(path) {
            let named = tops(found, &roots);
            let from = top(path);
            for to in named
                .into_iter()
                .filter(|to| *to != from && modules.contains(to))
            {
                edges.insert((from.clone(), to));
            }
        }
    }
    for (from, called) in calls {
        for to in called.iter().filter_map(|name| owner.get(name)) {
            if *to != from {
                edges.insert((from.clone(), to.clone()));
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
    for (domain, modules) in map.modules.iter().filter(|(domain, _)| *domain != list) {
        for module in modules
            .iter()
            .filter(|m| named.contains(*m) && *m != "load")
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
    let named: BTreeSet<String> = edges.iter().map(|(a, b)| format!("{a} -> {b}")).collect();
    for edge in listed.difference(&upward) {
        problems.push(listed_problem(edge, named.contains(edge), "upward"));
    }
    problems
}

/// Why `edge`, listed in the map's `list`, is wrong: it is in the code but
/// does not go up the order, or it is gone from the code.
fn listed_problem(edge: &str, in_code: bool, list: &str) -> String {
    if in_code {
        format!("{MAP}: {edge} does not go up the domain order; drop it from {list}")
    } else {
        format!("{MAP}: {edge} is gone from the code; drop it from {list}")
    }
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

fn roots(lib: &str) -> Result<BTreeMap<String, String>, String> {
    let file = syn::parse_file(lib).map_err(|e| format!("{SOURCES}/lib.rs: {e}"))?;
    let mut roots = BTreeMap::new();
    for item in &file.items {
        if let Item::Use(item) = item
            && matches!(item.vis, Visibility::Public(_))
        {
            for (name, full) in use_leaves(&item.tree) {
                let from = full.iter().find(|s| *s != "crate" && *s != "self");
                if let (Some(name), Some(from)) = (name, from)
                    && name != *from
                {
                    roots.insert(name, from.clone());
                }
            }
        }
    }
    Ok(roots)
}

/// Each name a `use` tree brings in with the path it stands for, as
/// written: `{self}` binds its module's own name, and a glob is `None` with
/// the path of the module it takes whole.
fn use_leaves(tree: &UseTree) -> Vec<(Option<String>, Vec<String>)> {
    fn walk(
        tree: &UseTree,
        prefix: &mut Vec<String>,
        out: &mut Vec<(Option<String>, Vec<String>)>,
    ) {
        match tree {
            UseTree::Path(path) => {
                prefix.push(path.ident.to_string());
                walk(&path.tree, prefix, out);
                prefix.pop();
            }
            UseTree::Name(name) if name.ident == "self" => {
                if let Some(last) = prefix.last() {
                    out.push((Some(last.clone()), prefix.clone()));
                }
            }
            UseTree::Name(name) => {
                let mut full = prefix.clone();
                full.push(name.ident.to_string());
                out.push((Some(name.ident.to_string()), full));
            }
            UseTree::Rename(rename) => {
                let mut full = prefix.clone();
                if rename.ident != "self" {
                    full.push(rename.ident.to_string());
                }
                out.push((Some(rename.rename.to_string()), full));
            }
            UseTree::Glob(_) => out.push((None, prefix.clone())),
            UseTree::Group(group) => {
                for tree in &group.items {
                    walk(tree, prefix, out);
                }
            }
        }
    }
    let mut out = Vec::new();
    walk(tree, &mut Vec::new(), &mut out);
    out
}

/// Leaves out the files of modules that exist only in tests, declared
/// `#[cfg(test)] mod name;`, with the modules under them.
fn drop_test_modules(files: &mut Vec<(String, String)>) {
    let mut test_modules = Vec::new();
    for (path, text) in files.iter() {
        // A file that does not parse fails where it is read for its paths.
        let Ok(file) = syn::parse_file(text) else {
            continue;
        };
        for item in &file.items {
            if let Item::Mod(declared) = item
                && declared.content.is_none()
                && test_only(item)
            {
                let mut module = module_path(path);
                module.push(declared.ident.to_string());
                test_modules.push(module);
            }
        }
    }
    files.retain(|(path, _)| {
        let module = module_path(path);
        !test_modules.iter().any(|test| module.starts_with(test))
    });
}

/// The top-level module each of `found`, absolute paths under core's root,
/// names, a root re-export read through `roots`.
fn tops(
    found: impl IntoIterator<Item = Vec<String>>,
    roots: &BTreeMap<String, String>,
) -> BTreeSet<String> {
    found
        .into_iter()
        .filter_map(|abs| {
            let first = abs.into_iter().next()?;
            Some(roots.get(&first).cloned().unwrap_or(first))
        })
        .collect()
}

/// The paths a file names, by where they start.
#[derive(Debug, Default)]
struct Named {
    /// Those under its crate's root, absolute.
    found: BTreeSet<Vec<String>>,
    /// Those that start at one of the crates the walk counts as external.
    external: BTreeSet<Vec<String>>,
    /// The rest, as written: a name the file imports or defines, a local
    /// or a prelude item.
    relative: BTreeSet<Vec<String>>,
}

/// The paths `file`, of module `module`, names, with those that start at
/// one of `externs` kept apart; `#[cfg(test)]` items left out, macro bodies
/// read as tokens.
fn paths(file: &syn::File, module: &[String], externs: &[&str]) -> Named {
    let mut walk = Walk::new(module, externs);
    walk.visit_file(file);
    walk.named
}

/// The paths of [`paths`] that no item under a `#[cfg(feature = "...")]`
/// holds, read through items, impl items and trait items.
fn ungated_paths(file: &syn::File, module: &[String], externs: &[&str]) -> Named {
    let mut walk = Walk::new(module, externs);
    walk.ungated = true;
    walk.visit_file(file);
    walk.named
}
/// The walk of [`paths`] over one file.
struct Walk<'a> {
    /// The module the walk stands in, inline modules counted.
    module: Vec<String>,
    /// The crates whose paths count as external.
    externs: &'a [&'a str],
    /// Whether the walk leaves out items under a `cfg(feature)`.
    ungated: bool,
    named: Named,
}

impl<'a> Walk<'a> {
    fn new(module: &[String], externs: &'a [&'a str]) -> Self {
        Self {
            module: module.to_vec(),
            externs,
            ungated: false,
            named: Named::default(),
        }
    }

    /// Whether `segments` start at one of the external crates.
    fn external(&self, segments: &[String]) -> bool {
        segments
            .first()
            .is_some_and(|first| self.externs.contains(&first.as_str()))
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

    /// Records the path of `segments` by where it starts.
    fn path(&mut self, segments: &[String]) {
        if let Some(absolute) = self.resolve(segments) {
            self.named.found.insert(absolute);
        } else if self.external(segments) {
            self.named.external.insert(segments.to_vec());
        } else if !segments.is_empty() {
            self.named.relative.insert(segments.to_vec());
        }
    }

    /// Records each path of a `use` tree.
    fn tree(&mut self, tree: &UseTree) {
        for (_, full) in use_leaves(tree) {
            self.path(&full);
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
        if !test_only(item) && !(self.ungated && feature_only(item_attrs(item))) {
            visit::visit_item(self, item);
        }
    }

    fn visit_impl_item(&mut self, item: &'ast ImplItem) {
        let attrs: &[Attribute] = match item {
            ImplItem::Const(item) => &item.attrs,
            ImplItem::Fn(item) => &item.attrs,
            ImplItem::Type(item) => &item.attrs,
            ImplItem::Macro(item) => &item.attrs,
            _ => &[],
        };
        if !(self.ungated && feature_only(attrs)) {
            visit::visit_impl_item(self, item);
        }
    }

    fn visit_trait_item(&mut self, item: &'ast TraitItem) {
        let attrs: &[Attribute] = match item {
            TraitItem::Const(item) => &item.attrs,
            TraitItem::Fn(item) => &item.attrs,
            TraitItem::Type(item) => &item.attrs,
            TraitItem::Macro(item) => &item.attrs,
            _ => &[],
        };
        if !(self.ungated && feature_only(attrs)) {
            visit::visit_trait_item(self, item);
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
        } else if self.external(&segments) {
            self.named.external.insert(segments);
        }
        visit::visit_path(self, path);
    }

    fn visit_use_tree(&mut self, tree: &'ast UseTree) {
        self.tree(tree);
    }

    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        self.tokens(mac.tokens.clone());
        visit::visit_macro(self, mac);
    }
}

fn test_only(item: &Item) -> bool {
    item_attrs(item).iter().any(|attr| {
        attr.path().is_ident("cfg") && attr.parse_args::<Ident>().is_ok_and(|arg| arg == "test")
    })
}

/// The attributes of `item`; none for a kind that never carries a `cfg`
/// here.
fn item_attrs(item: &Item) -> &[Attribute] {
    match item {
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
        _ => &[],
    }
}

/// Whether `attrs` hold a plain `#[cfg(feature = "...")]`; a compound
/// `cfg` counts as none, so a reader of it errs towards ungated.
fn feature_only(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path().is_ident("cfg")
            && attr
                .parse_args::<MetaNameValue>()
                .is_ok_and(|meta| meta.path.is_ident("feature"))
    })
}

#[cfg(test)]
mod tests;
