//! The domain rule in the fake loader: each file under `test/` and
//! `test/table/` belongs to the domain its folder or file is named after,
//! `test.rs` and `test/table.rs` join them, and a domain's files name only
//! its own domain and those before it, of the fake and of core.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use syn::visit::Visit;
use syn::{Item, Visibility};

use super::{
    MAP, Map, Walk, collect, drop_test_modules, listed_problem, module_path, package, paths,
    test_only, top, use_leaves,
};

/// The fake loader's sources, from the workspace root.
pub(super) const SOURCES: &str = "crates/internal/fake/src";
/// The fake's module under its `src`, whose children are the domains.
pub(super) const ROOT: &str = "test";
/// The fake's `HostApi` table under [`ROOT`], whose children are the
/// domains too.
const TABLE: &str = "table";
/// The name the fake knows core by.
pub(super) const CORE_CRATE: &str = "enshwick_core";
/// Core's module of what the fake reads inside it, under core's `src`.
const CORE_FAKE: &str = "__fake";

/// The `[fake]` table of `domains.toml`.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Fake {
    /// The edges up the order still in the fake's code, as `from -> to`
    /// between domains.
    upward: Vec<String>,
}

/// What breaks the domain rule in the fake under `root`, one line each;
/// `core_roots` maps the items core's root re-exports to their modules.
pub(super) fn problems(
    root: &Path,
    map: &Map,
    core_roots: &BTreeMap<String, String>,
) -> Result<Vec<String>, String> {
    let sources = root.join(SOURCES);
    let mut files = Vec::new();
    collect(&sources, &sources, &mut files)?;
    files
        .retain(|(path, _)| path == &format!("{ROOT}.rs") || path.starts_with(&format!("{ROOT}/")));
    drop_test_modules(&mut files);
    files.sort();
    let mut parsed = Vec::new();
    for (path, text) in &files {
        let file = syn::parse_file(text).map_err(|e| format!("{SOURCES}/{path}: {e}"))?;
        parsed.push((path.as_str(), file));
    }
    let resolver = Resolver::new(root, map, core_roots, &parsed)?;
    let externs: Vec<&str> = std::iter::once(CORE_CRATE)
        .chain(resolver.crates.keys().map(String::as_str))
        .collect();
    let rank = |domain: &str| map.order.iter().position(|d| d == domain);
    let mut problems = Vec::new();
    // Each edge between two domains with the first file that has it and
    // the path it names.
    let mut edges = BTreeMap::new();
    for (path, file) in parsed.iter().filter(|(path, _)| !is_join(path)) {
        let Some(from) = domain(path) else {
            continue;
        };
        if rank(&from).is_none() {
            problems.push(format!(
                "{SOURCES}/{path} names no domain of {MAP}: move it into a domain's folder"
            ));
            continue;
        }
        let module = module_path(path);
        let named = paths(file, &module, &externs);
        let (own, _) = imports(file, &module);
        let relative = named.relative.iter().filter_map(|segments| {
            let (first, rest) = segments.split_first()?;
            Some(own.get(first)?.iter().chain(rest).cloned().collect())
        });
        let local = named.found.iter().map(|abs| {
            std::iter::once("crate".to_owned())
                .chain(abs.iter().cloned())
                .collect()
        });
        for full in local.chain(named.external.iter().cloned()).chain(relative) {
            let Some(to) = resolver.domain(full.clone()) else {
                continue;
            };
            if to != from && rank(&to).is_some() {
                edges
                    .entry((from.clone(), to))
                    .or_insert_with(|| (path.to_string(), full.join("::")));
            }
        }
    }
    let mut upward = BTreeMap::new();
    for ((from, to), first) in &edges {
        if rank(to) > rank(from) {
            upward.insert(format!("{from} -> {to}"), first);
        }
    }
    let listed: BTreeSet<String> = map.fake.upward.iter().cloned().collect();
    for (edge, (path, named)) in upward.iter().filter(|(edge, _)| !listed.contains(*edge)) {
        problems.push(format!(
            "{SOURCES}/{path}: {edge} goes up the domain order ({named}): move what it names down into base or a lower domain"
        ));
    }
    let in_code: BTreeSet<String> = edges.keys().map(|(a, b)| format!("{a} -> {b}")).collect();
    for edge in listed.iter().filter(|edge| !upward.contains_key(*edge)) {
        problems.push(listed_problem(
            &format!("the fake's {edge}"),
            in_code.contains(edge),
            "[fake] upward",
        ));
    }
    Ok(problems)
}

/// Whether `path`, under the fake's `src`, is one of the two joins.
fn is_join(path: &str) -> bool {
    path == format!("{ROOT}.rs") || path == format!("{ROOT}/{TABLE}.rs")
}

/// The domain of a fake file under `src`, by its folder or file name under
/// `test/` or `test/table/`; `None` outside them.
fn domain(path: &str) -> Option<String> {
    let under = path.strip_prefix(ROOT)?.strip_prefix('/')?;
    let under = under
        .strip_prefix(TABLE)
        .and_then(|rest| rest.strip_prefix('/'))
        .unwrap_or(under);
    Some(top(under))
}

/// Where a path the fake names leads: a path under the fake's root starts
/// at `crate`, any other at the crate it names.
struct Resolver<'a> {
    /// What each join imports by name, as `<join module>::<name>`.
    joined: BTreeMap<Vec<String>, Vec<String>>,
    /// The modules each join's glob imports take whole, by the join.
    globs: BTreeMap<Vec<String>, Vec<Vec<String>>>,
    /// The modules each join declares, by the join.
    mods: BTreeMap<Vec<String>, BTreeSet<String>>,
    /// The items each join defines, by the join.
    own: BTreeMap<Vec<String>, BTreeSet<String>>,
    /// The names a glob of each fake module takes: those it defines and
    /// those it re-exports, by the module.
    exported: BTreeMap<Vec<String>, BTreeSet<String>>,
    /// Core's module of each item its root re-exports.
    core_roots: &'a BTreeMap<String, String>,
    /// The domain of each module still in core.
    core_modules: BTreeMap<String, String>,
    /// The domain of each peeled domain's crate, by the name code knows it
    /// by.
    crates: BTreeMap<String, String>,
    /// The domain of each item of core's `__fake`, by the core modules it
    /// comes from or names.
    core_fake: BTreeMap<String, String>,
}

impl<'a> Resolver<'a> {
    fn new(
        root: &Path,
        map: &Map,
        core_roots: &'a BTreeMap<String, String>,
        files: &[(&str, syn::File)],
    ) -> Result<Self, String> {
        let mut crates = BTreeMap::new();
        let mut peeled_fake = BTreeMap::new();
        for (domain, dir) in &map.crates {
            let (_, name) = package(root, dir)?;
            let name = name.replace('-', "_");
            let source = format!("crates/internal/{dir}/src/{CORE_FAKE}.rs");
            if let Ok(text) = std::fs::read_to_string(root.join(&source)) {
                let file = syn::parse_file(&text).map_err(|e| format!("{source}: {e}"))?;
                peeled_fake.insert(name.clone(), exported(&file));
            }
            crates.insert(name, domain.clone());
        }
        let core_modules = map
            .modules
            .iter()
            .flat_map(|(domain, modules)| modules.iter().map(move |m| (m.clone(), domain.clone())))
            .collect();
        let mut resolver = Self {
            joined: BTreeMap::new(),
            globs: BTreeMap::new(),
            mods: BTreeMap::new(),
            own: BTreeMap::new(),
            exported: BTreeMap::new(),
            core_roots,
            core_modules,
            crates,
            core_fake: BTreeMap::new(),
        };
        for (path, file) in files {
            let module = module_path(path);
            let (names, globs) = imports(file, &module);
            resolver.exported.insert(module.clone(), exported(file));
            if is_join(path) {
                resolver.own.insert(module.clone(), defined(file));
                let mods = file
                    .items
                    .iter()
                    .filter_map(|item| match item {
                        Item::Mod(declared) if !test_only(item) => Some(declared.ident.to_string()),
                        _ => None,
                    })
                    .collect();
                resolver.mods.insert(module.clone(), mods);
                resolver.globs.insert(module.clone(), globs);
                for (name, full) in &names {
                    let mut key = module.clone();
                    key.push(name.clone());
                    resolver.joined.insert(key, full.clone());
                }
            }
        }
        let core_fake = root.join(super::SOURCES).join(format!("{CORE_FAKE}.rs"));
        if let Ok(text) = std::fs::read_to_string(&core_fake) {
            let file = syn::parse_file(&text)
                .map_err(|e| format!("{}/{CORE_FAKE}.rs: {e}", super::SOURCES))?;
            resolver.core_fake = resolver.core_fake_domains(&file, &map.order, &peeled_fake);
        }
        Ok(resolver)
    }

    /// The domain `path` leads to, `None` when it leads to none: a join's
    /// own item, a crate outside the map, or a name the check cannot follow.
    fn domain(&self, mut path: Vec<String>) -> Option<String> {
        // Each turn follows one import; imports do not cycle, so the bound
        // only guards against a malformed tree.
        for _ in 0..16 {
            if path.first().is_none_or(|first| first != "crate") {
                return self.extern_domain(&path);
            }
            if path.get(1).is_none_or(|s| s != ROOT) {
                return None;
            }
            let depth = if path.get(2).is_some_and(|s| s == TABLE) {
                3
            } else {
                2
            };
            let join = &path[1..depth];
            let name = path.get(depth)?;
            if self.mods.get(join).is_some_and(|mods| mods.contains(name)) {
                return Some(name.clone());
            }
            if self.own.get(join).is_some_and(|own| own.contains(name)) {
                return None;
            }
            let rest = &path[depth + 1..];
            let mut key = join.to_vec();
            key.push(name.clone());
            let next = if let Some(full) = self.joined.get(&key) {
                full.iter().chain(rest).cloned().collect()
            } else {
                let glob = self.globs.get(join)?.iter().find(|glob| {
                    glob.split_first().is_some_and(|(first, module)| {
                        first == "crate"
                            && self.exported.get(module).is_some_and(|e| e.contains(name))
                    })
                })?;
                glob.iter()
                    .chain(std::iter::once(name))
                    .chain(rest)
                    .cloned()
                    .collect()
            };
            path = next;
        }
        None
    }

    /// The domain of a path that starts at another crate.
    fn extern_domain(&self, path: &[String]) -> Option<String> {
        let (krate, rest) = path.split_first()?;
        if let Some(domain) = self.crates.get(krate) {
            return Some(domain.clone());
        }
        if krate != CORE_CRATE {
            return None;
        }
        let first = rest.first()?;
        if first == CORE_FAKE {
            return self.core_fake.get(rest.get(1)?).cloned();
        }
        self.core_module_domain(first)
    }

    /// The domain of what core's root names `first`: one of its modules, an
    /// item it re-exports or a peeled domain's crate.
    fn core_module_domain(&self, first: &str) -> Option<String> {
        let module = self.core_roots.get(first).map_or(first, String::as_str);
        self.core_modules
            .get(module)
            .or_else(|| self.crates.get(module))
            .cloned()
    }

    /// The domain of each public item of core's `__fake`: a re-export's by
    /// the module it comes from, an item of a peeled crate's `__fake` that
    /// it takes whole by that crate (`peeled_fake`, the names each one
    /// hands down, by the crate), any other item's the highest of the core
    /// modules it names; an item that names none stays out.
    fn core_fake_domains(
        &self,
        file: &syn::File,
        order: &[String],
        peeled_fake: &BTreeMap<String, BTreeSet<String>>,
    ) -> BTreeMap<String, String> {
        let module = [CORE_FAKE.to_owned()];
        let (own, _) = imports(file, &module);
        let rank = |domain: &String| order.iter().position(|d| d == domain);
        let of = |full: &[String]| match full.split_first() {
            Some((first, rest)) if first == "crate" => self.core_module_domain(rest.first()?),
            Some((first, _)) => self.crates.get(first).cloned(),
            None => None,
        };
        let mut domains = BTreeMap::new();
        for item in file.items.iter().filter(|item| !test_only(item)) {
            if let Item::Use(used) = item {
                if matches!(used.vis, Visibility::Public(_)) {
                    for (name, full) in use_leaves(&used.tree) {
                        match name {
                            Some(name) => {
                                if let Some(domain) = own.get(&name).and_then(|full| of(full)) {
                                    domains.insert(name, domain);
                                }
                            }
                            // A peeled domain's own `__fake`, taken whole.
                            None => {
                                if let [krate, module] = full.as_slice()
                                    && module == CORE_FAKE
                                    && let Some(domain) = self.crates.get(krate)
                                {
                                    for name in peeled_fake.get(krate).into_iter().flatten() {
                                        domains.insert(name.clone(), domain.clone());
                                    }
                                }
                            }
                        }
                    }
                }
                continue;
            }
            let Some(name) = ident(item) else {
                continue;
            };
            let mut walk = Walk::new(&module, &[]);
            walk.visit_item(item);
            let local = walk.named.found.iter().map(|abs| {
                std::iter::once("crate".to_owned())
                    .chain(abs.iter().cloned())
                    .collect::<Vec<_>>()
            });
            let relative = walk.named.relative.iter().filter_map(|segments| {
                let (first, rest) = segments.split_first()?;
                Some(
                    own.get(first)?
                        .iter()
                        .chain(rest)
                        .cloned()
                        .collect::<Vec<_>>(),
                )
            });
            if let Some(domain) = local
                .chain(relative)
                .filter_map(|full| of(&full))
                .max_by_key(|domain| rank(domain))
            {
                domains.insert(name, domain);
            }
        }
        domains
    }
}

/// The name an item defines, for those a path can name.
fn ident(item: &Item) -> Option<String> {
    let ident = match item {
        Item::Const(item) => &item.ident,
        Item::Enum(item) => &item.ident,
        Item::Fn(item) => &item.sig.ident,
        Item::Mod(item) => &item.ident,
        Item::Static(item) => &item.ident,
        Item::Struct(item) => &item.ident,
        Item::Trait(item) => &item.ident,
        Item::Type(item) => &item.ident,
        Item::Union(item) => &item.ident,
        Item::Macro(item) => item.ident.as_ref()?,
        _ => return None,
    };
    Some(ident.to_string())
}

/// The names the items of `file` define, tests left out.
fn defined(file: &syn::File) -> BTreeSet<String> {
    file.items
        .iter()
        .filter(|item| !test_only(item))
        .filter_map(ident)
        .collect()
}

/// The names a glob of `file`'s module takes: those it defines and those it
/// re-exports.
fn exported(file: &syn::File) -> BTreeSet<String> {
    let mut names = defined(file);
    for item in file.items.iter().filter(|item| !test_only(item)) {
        if let Item::Use(used) = item
            && !matches!(used.vis, Visibility::Inherited)
        {
            names.extend(
                use_leaves(&used.tree)
                    .into_iter()
                    .filter_map(|(name, _)| name),
            );
        }
    }
    names
}

/// What the `use` items of `file`, of module `module`, bring in: each name
/// with the path it stands for, under the fake's root as `crate::…`, at
/// another crate as written, and the modules its globs take whole.
///
/// shortcut: inline modules' own `use` items count as the file's; an
/// inline module that imports a name its file uses otherwise would be
/// misread, so split it into a file of its own.
fn imports(
    file: &syn::File,
    module: &[String],
) -> (BTreeMap<String, Vec<String>>, Vec<Vec<String>>) {
    let local = defined(file);
    let walk = Walk::new(module, &[]);
    let mut names = BTreeMap::new();
    let mut globs = Vec::new();
    for item in file.items.iter().filter(|item| !test_only(item)) {
        let Item::Use(used) = item else {
            continue;
        };
        for (name, full) in use_leaves(&used.tree) {
            let absolute = match walk.resolve(&full) {
                Some(abs) => std::iter::once("crate".to_owned()).chain(abs).collect(),
                // A plain path starts at an item of the module, else at a
                // crate.
                None if full.first().is_some_and(|first| local.contains(first)) => {
                    std::iter::once("crate".to_owned())
                        .chain(module.iter().cloned())
                        .chain(full)
                        .collect()
                }
                None => full,
            };
            match name {
                Some(name) => {
                    names.insert(name, absolute);
                }
                None => globs.push(absolute),
            }
        }
    }
    (names, globs)
}

#[cfg(test)]
mod tests {
    use super::super::tests::{Tree, module};
    use super::*;

    #[test]
    fn a_fake_file_takes_the_domain_of_its_folder_under_test_or_table() {
        assert_eq!(domain("test/world.rs").as_deref(), Some("world"));
        assert_eq!(domain("test/world/props.rs").as_deref(), Some("world"));
        assert_eq!(domain("test/table/ui.rs").as_deref(), Some("ui"));
        assert_eq!(domain("test/table/base/mods.rs").as_deref(), Some("base"));
        assert_eq!(domain("lib.rs"), None);
    }

    #[test]
    fn imports_bind_self_to_its_module_and_keep_other_crates_as_written() -> Result<(), syn::Error>
    {
        let file = syn::parse_file(
            "mod world;\nuse world::players::{self, ask as asked};\nuse enshwick_core::abi::Side;\nuse super::table::*;",
        )?;
        let (names, globs) = imports(&file, &module("test"));
        let path = |name: &str| names.get(name).map(|full| full.join("::"));
        assert_eq!(
            path("players").as_deref(),
            Some("crate::test::world::players")
        );
        assert_eq!(
            path("asked").as_deref(),
            Some("crate::test::world::players::ask")
        );
        assert_eq!(path("Side").as_deref(), Some("enshwick_core::abi::Side"));
        assert_eq!(globs, [module("crate::table")]);
        Ok(())
    }

    /// A tree of core and the fake under a temporary root, its core
    /// `__fake.rs` handing down a ui item, a world one and the `__fake` of
    /// a peeled `content` crate.
    fn tree(name: &str) -> Tree {
        let root = std::env::temp_dir().join(format!("xtask-fake-{name}-{}", std::process::id()));
        let _gone = std::fs::remove_dir_all(&root);
        let tree = Tree(root);
        tree.write(
            "crates/internal/content/Cargo.toml",
            "[package]\nname = \"enshwick-content\"\n",
        );
        tree.write(
            &format!("{}/{CORE_FAKE}.rs", super::super::SOURCES),
            "use crate::voxels::Voxel;\npub use enshwick_content::__fake::*;\npub use crate::ui::screens::NAMES;\npub fn raw(v: Voxel) {}\npub fn plain() {}",
        );
        tree.write(
            &format!("crates/internal/content/src/{CORE_FAKE}.rs"),
            "pub fn combine() {}",
        );
        tree
    }

    fn map(upward: &str) -> Map {
        let text = format!(
            "order = [\"base\", \"content\", \"world\", \"ui\"]\njoins = []\nupward = []\n[crates]\ncontent = \"content\"\n[modules]\nbase = [\"host\"]\nworld = [\"map\", \"voxels\"]\nui = [\"ui\"]\n[fake]\nupward = [{upward}]\n"
        );
        toml::from_str(&text).unwrap_or_else(|e| panic!("{e}"))
    }

    fn check(tree: &Tree, upward: &str) -> Vec<String> {
        let roots = [("Peeled".to_owned(), "enshwick_content".to_owned())].into();
        problems(&tree.0, &map(upward), &roots).unwrap_or_else(|e| vec![e])
    }

    fn fake(tree: &Tree, path: &str, text: &str) {
        tree.write(&format!("{SOURCES}/{path}"), text);
    }

    #[test]
    fn an_edge_up_through_any_path_the_fake_writes_fails() {
        let tree = tree("up");
        fake(
            &tree,
            "test.rs",
            "mod base; mod content; mod world; mod ui; mod table;\npub use ui::Drawn;\nuse enshwick_core::ui::Frame;\nuse world::players::{self};",
        );
        fake(
            &tree,
            "test/table.rs",
            "mod base; mod ui;\nuse ui::*;\npub(crate) fn with() {}",
        );
        fake(
            &tree,
            "test/table/ui.rs",
            "use crate::test::table::with;\npub(crate) fn frame() {}",
        );
        fake(&tree, "test/ui.rs", "pub struct Drawn;");
        fake(&tree, "test/world/players.rs", "pub fn ask() {}");
        let cases = [
            ("join re-export", "use crate::test::Drawn;", "ui"),
            ("join import of core", "use crate::test::Frame;", "ui"),
            (
                "self in a join's group",
                "fn f() { crate::test::players::ask(); }",
                "world",
            ),
            (
                "imported join module",
                "use crate::test::table;\nfn f() { table::ui::frame(); }",
                "ui",
            ),
            ("join glob", "fn f() { crate::test::table::frame(); }", "ui"),
            (
                "leading colons",
                "fn f() { ::enshwick_core::ui::Frame::new(); }",
                "ui",
            ),
            (
                "peeled crate",
                "fn f() { enshwick_content::x(); }",
                "content",
            ),
            (
                "core re-export of a peeled crate",
                "fn f() { enshwick_core::Peeled::x(); }",
                "content",
            ),
            (
                "core's fake re-export",
                "use enshwick_core::__fake::NAMES;",
                "ui",
            ),
            (
                "core's fake item",
                "fn f() { enshwick_core::__fake::raw(); }",
                "world",
            ),
            (
                "a peeled crate's fake item core hands down",
                "fn f() { enshwick_core::__fake::combine(); }",
                "content",
            ),
        ];
        for (case, code, to) in cases {
            fake(&tree, "test/table/base/case.rs", code);
            let problems = check(&tree, "");
            assert!(
                problems.len() == 1
                    && problems[0]
                        .contains(&format!("test/table/base/case.rs: base -> {to} goes up")),
                "{case}: {problems:?}"
            );
        }
        let passing = [
            (
                "an item that names no module",
                "fn f() { enshwick_core::__fake::plain(); }",
            ),
            (
                "the join's own item a glob's module imports privately",
                "fn f() { crate::test::table::with(); }",
            ),
        ];
        for (case, code) in passing {
            fake(&tree, "test/table/base/case.rs", code);
            assert_eq!(check(&tree, ""), Vec::<String>::new(), "{case}");
        }
    }

    #[test]
    fn a_test_module_file_is_left_out_and_a_stray_file_fails() {
        let tree = tree("tests");
        fake(&tree, "test.rs", "mod base; mod ui;");
        fake(&tree, "test/base.rs", "#[cfg(test)]\nmod checks;");
        fake(
            &tree,
            "test/base/checks.rs",
            "use enshwick_core::ui::Frame;",
        );
        fake(&tree, "test/sundry.rs", "");
        let problems = check(&tree, "");
        assert!(
            problems.len() == 1 && problems[0].contains("test/sundry.rs names no domain"),
            "{problems:?}"
        );
    }

    #[test]
    fn a_listed_edge_says_whether_it_is_gone_or_only_not_upward() {
        let tree = tree("listed");
        fake(&tree, "test.rs", "mod base; mod world; mod ui;");
        fake(
            &tree,
            "test/world.rs",
            "use crate::test::base::B;\nuse crate::test::ui::U;",
        );
        assert_eq!(check(&tree, "\"world -> ui\""), Vec::<String>::new());
        let problems = check(&tree, "\"world -> ui\", \"world -> base\", \"base -> ui\"");
        assert!(
            problems
                .iter()
                .any(|p| p.contains("the fake's world -> base does not go up")),
            "{problems:?}"
        );
        assert!(
            problems
                .iter()
                .any(|p| p.contains("the fake's base -> ui is gone")),
            "{problems:?}"
        );
        assert_eq!(problems.len(), 2, "{problems:?}");
    }
}
