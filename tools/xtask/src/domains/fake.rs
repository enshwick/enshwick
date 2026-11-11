//! The domain rule in the fake loader: each file under `test/` and
//! `test/table/` belongs to the domain its folder or file is named after,
//! `test.rs` and `test/table.rs` join them, and a domain's files name only
//! its own domain and those before it, of the fake and of core.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use syn::{Item, UseTree};

use super::{MAP, Map, Walk, collect, module_path, paths, test_only, top};

/// The fake loader's sources, from the workspace root.
pub(super) const SOURCES: &str = "crates/internal/fake/src";
/// The fake's module under its `src`, whose children are the domains.
pub(super) const ROOT: &str = "test";
/// The fake's `HostApi` table under [`ROOT`], whose children are the
/// domains too.
const TABLE: &str = "table";
/// The name the fake knows core by.
pub(super) const CORE_CRATE: &str = "enshwick_core";

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
    collect(&sources, &sources.join(ROOT), &mut files)?;
    let joins = [format!("{ROOT}.rs"), format!("{ROOT}/{TABLE}.rs")];
    let joined = std::fs::read_to_string(sources.join(&joins[0]))
        .map_err(|e| format!("{SOURCES}/{}: {e}", joins[0]))?;
    files.push((joins[0].clone(), joined));
    let mut imported = BTreeMap::new();
    for (path, text) in files.iter().filter(|(path, _)| joins.contains(path)) {
        let found =
            imports(text, &module_path(path)).map_err(|e| format!("{SOURCES}/{path}: {e}"))?;
        imported.extend(found);
    }
    let domain_of: BTreeMap<&str, &str> = map
        .modules
        .iter()
        .flat_map(|(domain, modules)| modules.iter().map(move |m| (m.as_str(), domain.as_str())))
        .collect();
    let rank = |domain: &str| map.order.iter().position(|d| d == domain);
    let mut problems = Vec::new();
    // Each edge up the order with the first file that has it.
    let mut upward = BTreeMap::new();
    for (path, text) in files.iter().filter(|(path, _)| !joins.contains(path)) {
        let Some(from) = domain(path) else {
            continue;
        };
        let Some(from_rank) = rank(&from) else {
            problems.push(format!(
                "{SOURCES}/{path} names no domain of {MAP}: move it into a domain's folder"
            ));
            continue;
        };
        let (named, external) = paths(text, &module_path(path), &[CORE_CRATE])
            .map_err(|e| format!("{SOURCES}/{path}: {e}"))?;
        let fake = named.iter().filter_map(|abs| {
            Some((
                target(abs, &imported)?,
                format!("crate::{}", abs.join("::")),
            ))
        });
        let core = external.iter().filter_map(|ext| {
            let first = ext.get(1)?;
            let module = core_roots.get(first).unwrap_or(first);
            let domain = domain_of.get(module.as_str())?;
            Some(((*domain).to_owned(), ext.join("::")))
        });
        for (to, named) in fake.chain(core) {
            if rank(&to).is_some_and(|to_rank| to_rank > from_rank) {
                upward
                    .entry(format!("{from} -> {to}"))
                    .or_insert_with(|| (path.clone(), named));
            }
        }
    }
    let listed: BTreeSet<String> = map.fake.upward.iter().cloned().collect();
    for (edge, (path, named)) in upward.iter().filter(|(edge, _)| !listed.contains(*edge)) {
        problems.push(format!(
            "{SOURCES}/{path}: {edge} goes up the domain order ({named}): move what it names down into base or a lower domain"
        ));
    }
    for edge in listed.iter().filter(|edge| !upward.contains_key(*edge)) {
        problems.push(format!(
            "{MAP}: the fake's {edge} is gone from the code; drop it from [fake] upward"
        ));
    }
    Ok(problems)
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

/// The fake domain the absolute path `abs` names, through the joins'
/// imports: `crate::test::<domain>` or `crate::test::table::<domain>`, or
/// an item a join imports from one.
///
/// shortcut: a name a join takes through a glob import is not followed;
/// write the domain's path instead, until the check needs to follow globs.
fn target(abs: &[String], imported: &BTreeMap<String, Vec<String>>) -> Option<String> {
    let mut abs = abs.to_vec();
    // Each step follows one import; a join's imports never cycle, so the
    // bound only guards against a malformed tree.
    for _ in 0..8 {
        let joined_depth = if abs.get(1).is_some_and(|s| s == TABLE) {
            2
        } else {
            1
        };
        if abs.first().is_none_or(|s| s != ROOT) || abs.len() <= joined_depth {
            return None;
        }
        let key = format!("{}::{}", abs[..joined_depth].join("::"), abs[joined_depth]);
        match imported.get(&key) {
            Some(target) => {
                abs = target
                    .iter()
                    .chain(&abs[joined_depth + 1..])
                    .cloned()
                    .collect();
            }
            None => return Some(abs[joined_depth].clone()),
        }
    }
    None
}

/// The names the `use` items of `text`, a file of module `module`, bring
/// into it, as `<module>::<name>` with the absolute path each stands for;
/// globs left out.
fn imports(text: &str, module: &[String]) -> Result<BTreeMap<String, Vec<String>>, String> {
    fn leaves(tree: &UseTree, prefix: &mut Vec<String>, out: &mut Vec<(String, Vec<String>)>) {
        match tree {
            UseTree::Path(path) => {
                prefix.push(path.ident.to_string());
                leaves(&path.tree, prefix, out);
                prefix.pop();
            }
            UseTree::Name(name) => {
                let mut full = prefix.clone();
                full.push(name.ident.to_string());
                out.push((name.ident.to_string(), full));
            }
            UseTree::Rename(rename) => {
                let mut full = prefix.clone();
                full.push(rename.ident.to_string());
                out.push((rename.rename.to_string(), full));
            }
            UseTree::Group(group) => {
                for tree in &group.items {
                    leaves(tree, prefix, out);
                }
            }
            UseTree::Glob(_) => {}
        }
    }
    let file = syn::parse_file(text).map_err(|e| e.to_string())?;
    let walk = Walk::new(module, &[]);
    let mut found = BTreeMap::new();
    for item in file.items.iter().filter(|item| !test_only(item)) {
        if let Item::Use(item) = item {
            let mut out = Vec::new();
            leaves(&item.tree, &mut Vec::new(), &mut out);
            for (name, segments) in out {
                // A plain path in a `use` starts at the module's own items.
                let absolute = walk
                    .resolve(&segments)
                    .unwrap_or_else(|| module.iter().chain(&segments).cloned().collect());
                found.insert(format!("{}::{name}", module.join("::")), absolute);
            }
        }
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn module(path: &str) -> Vec<String> {
        path.split("::").map(str::to_owned).collect()
    }

    #[test]
    fn a_fake_file_takes_the_domain_of_its_folder_under_test_or_table() {
        assert_eq!(domain("test/world.rs").as_deref(), Some("world"));
        assert_eq!(domain("test/world/props.rs").as_deref(), Some("world"));
        assert_eq!(domain("test/table/ui.rs").as_deref(), Some("ui"));
        assert_eq!(domain("test/table/base/mods.rs").as_deref(), Some("base"));
        assert_eq!(domain("lib.rs"), None);
    }

    #[test]
    fn a_fake_path_follows_the_joins_imports_to_its_domain() -> Result<(), String> {
        let mut imported = imports(
            "pub use base::hooks::LinkedHook;\nuse world::players::{ask as asked};\nuse enshwick_core::abi::Side;",
            &module("test"),
        )?;
        imported.extend(imports(
            "use super::FakeHost;\npub(crate) use base::services::view;",
            &module("test::table"),
        )?);
        let target = |path: &str| target(&module(path), &imported);
        assert_eq!(target("test::LinkedHook").as_deref(), Some("base"));
        assert_eq!(target("test::asked").as_deref(), Some("world"));
        assert_eq!(target("test::table::view").as_deref(), Some("base"));
        assert_eq!(target("test::table::ui::frame").as_deref(), Some("ui"));
        assert_eq!(
            target("test::inventory::stats").as_deref(),
            Some("inventory")
        );
        assert_eq!(target("test::FakeHost").as_deref(), Some("FakeHost"));
        let renamed = imports("use self::table as joined;", &module("test"))?;
        assert_eq!(
            super::target(&module("test::joined::world::x"), &renamed).as_deref(),
            Some("world"),
            "the path past an imported name follows it"
        );
        assert_eq!(target("other::ui"), None);
        Ok(())
    }

    #[test]
    fn a_fake_tree_with_an_edge_up_to_the_fake_or_to_core_fails() {
        /// Removes the tree however the test ends.
        struct Tree(std::path::PathBuf);
        impl Drop for Tree {
            fn drop(&mut self) {
                let _gone = std::fs::remove_dir_all(&self.0);
            }
        }
        let root = std::env::temp_dir().join(format!("xtask-fake-{}", std::process::id()));
        let _gone = std::fs::remove_dir_all(&root);
        let tree = Tree(root.clone());
        let write = |path: &str, text: &str| {
            let path = tree.0.join(SOURCES).join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        };
        let map = |upward: &str| {
            toml::from_str::<Map>(&format!(
                "order = [\"base\", \"world\", \"ui\"]\njoins = []\nupward = []\n[modules]\nbase = [\"host\"]\nworld = [\"map\"]\nui = [\"ui\"]\n[fake]\nupward = [{upward}]\n"
            ))
            .unwrap()
        };
        write(
            "test.rs",
            "mod base; mod world; mod ui; mod table;\npub use ui::Drawn;",
        );
        write("test/table.rs", "mod world;");
        write("test/base.rs", "use crate::test::Drawn;");
        write(
            "test/table/world.rs",
            "use enshwick_core::map::Map;\nuse super::super::base::B;",
        );
        write("test/world/props.rs", "use enshwick_core::ui::Frame;");
        write("test/sundry.rs", "");
        let problems = problems(&root, &map(""), &BTreeMap::new()).unwrap();
        assert!(
            problems.iter().any(|p| p.ends_with("test/base.rs: base -> ui goes up the domain order (crate::test::Drawn): move what it names down into base or a lower domain")),
            "{problems:?}"
        );
        assert!(
            problems.iter().any(|p| p.contains("world -> ui goes up")),
            "{problems:?}"
        );
        assert!(
            problems
                .iter()
                .any(|p| p.contains("test/sundry.rs names no domain")),
            "{problems:?}"
        );
        assert_eq!(problems.len(), 3, "{problems:?}");
        let listed = super::problems(
            &root,
            &map("\"base -> ui\", \"world -> ui\", \"world -> base\""),
            &BTreeMap::new(),
        )
        .unwrap();
        assert!(
            listed
                .iter()
                .any(|p| p.contains("the fake's world -> base is gone")),
            "{listed:?}"
        );
        assert_eq!(listed.len(), 2, "{listed:?}");
    }
}
