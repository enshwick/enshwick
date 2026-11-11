//! The domain rule of `enshwick-core`: each top-level module belongs to one
//! domain of `crates/internal/core/domains.toml`, and a module names only
//! its own domain and those listed before it. The edges up that order still
//! in the code are listed there too; a new one, or a listed one that is
//! gone, fails, so the list only shrinks as domains are peeled out.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// The map, from the workspace root.
const MAP: &str = "crates/internal/core/domains.toml";
/// Core's sources, from the workspace root.
const SOURCES: &str = "crates/internal/core/src";

/// `domains.toml`.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Map {
    /// The domains, lowest first.
    order: Vec<String>,
    /// Files and folders, under `src`, that join every domain's list.
    joins: Vec<String>,
    /// Each domain's top-level modules.
    modules: BTreeMap<String, Vec<String>>,
    /// The edges up the order still in the code, as `from -> to`.
    upward: Vec<String>,
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
    files.retain(|(path, _)| !map.joins.iter().any(|join| path.starts_with(join.as_str())));
    let modules: BTreeSet<String> = files.iter().map(|(path, _)| top(path)).collect();
    let mut edges = BTreeSet::new();
    for (path, text) in &files {
        let from = top(path);
        for to in names(&code(text), depth(path)) {
            if to != from && modules.contains(&to) {
                edges.insert((from.clone(), to));
            }
        }
    }
    problems.extend(judge(&map, &modules, &edges));
    Ok(problems)
}

/// The script namespaces' join: one list file per domain under
/// `script/namespaces/`, named after it, joined in the map's order.
fn namespace_lists(map: &Map, files: &[(String, String)]) -> Vec<String> {
    const LISTS: &str = "script/namespaces/";
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

/// `text` without comments, doc examples among them, and without a
/// trailing `#[cfg(test)] mod tests { .. }`.
fn code(text: &str) -> String {
    let cut = text
        .find("#[cfg(test)]\nmod tests {")
        .map_or(text, |at| &text[..at]);
    cut.lines()
        .map(|line| line.split_once("//").map_or(line, |(code, _)| code))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The module depth of a file under `src`: 1 for `ui.rs` and `ui/mod.rs`,
/// 2 for `ui/frame.rs`.
fn depth(path: &str) -> usize {
    let segments = path.split('/').count();
    if path.ends_with("/mod.rs") {
        segments - 1
    } else {
        segments
    }
}

/// The first segment under the crate root of every path in `code`, a file
/// `depth` modules deep: `crate::` paths and chains of `depth` `super::`,
/// those of a `{..}` group included.
///
/// shortcut: an inline `mod` deepens the file's own depth, so a chain inside
/// one is read one module short; parse with syn if that ever misleads.
fn names(code: &str, depth: usize) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let mut rest = code;
    while let Some(at) = rest
        .find("crate::")
        .into_iter()
        .chain(rest.find("super::"))
        .min()
    {
        let before = rest[..at].chars().next_back();
        rest = &rest[at..];
        let mut supers = 0;
        while let Some(after) = rest.strip_prefix("super::") {
            supers += 1;
            rest = after;
        }
        if supers == 0 {
            rest = &rest["crate::".len()..];
        }
        if before.is_some_and(|c| c.is_alphanumeric() || c == '_' || c == ':')
            || (supers > 0 && supers != depth)
        {
            continue;
        }
        segments(rest, &mut found);
    }
    found
}

/// The first segment of the path `rest` starts, or of each path in the
/// `{..}` group it starts.
fn segments(rest: &str, found: &mut BTreeSet<String>) {
    let Some(group) = rest.strip_prefix('{') else {
        found.extend(ident(rest));
        return;
    };
    let mut depth = 1;
    let end = group
        .char_indices()
        .find(|&(_, c)| {
            depth += match c {
                '{' => 1,
                '}' => -1,
                _ => 0,
            };
            depth == 0
        })
        .map_or(group.len(), |(i, _)| i);
    let mut level = 0;
    let mut start = 0;
    for (i, c) in group[..end].char_indices().chain([(end, ',')]) {
        match c {
            '{' => level += 1,
            '}' => level -= 1,
            ',' if level == 0 => {
                found.extend(ident(group[start..i].trim()));
                start = i + 1;
            }
            _ => {}
        }
    }
}

/// The lowercase identifier `text` starts with, unless a macro call.
fn ident(text: &str) -> Option<String> {
    let end = text
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .unwrap_or(text.len());
    let name = &text[..end];
    let lower = name.starts_with(|c: char| c.is_ascii_lowercase() || c == '_');
    (lower && !text[end..].starts_with('!')).then(|| name.to_owned())
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
        }
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
    fn names_reads_paths_groups_and_nested_groups_but_not_macros_or_roots() {
        let code = "use crate::map::Map;\nuse crate::{host::Host, ui::{Ui, frame}, Load};\ncrate::warn!(\"x\");\nlet a = foo_crate::x;";
        assert_eq!(names(code, 1), set(&["host", "map", "ui"]));
        let nested = "use crate::{ui::{Ui, frame::{A, b}}, sound::Sound};\nuse crate::time;";
        assert_eq!(names(nested, 1), set(&["sound", "time", "ui"]));
    }

    #[test]
    fn names_follows_a_super_chain_to_the_root_but_not_one_that_stays_inside() {
        let code = "use super::super::ui::Ui;\nuse super::frame::F;\nlet x = super::super::{host::H, map};\nuse self::super::sound;";
        assert_eq!(names(code, 2), set(&["host", "map", "ui"]));
        assert_eq!(names("use super::host::Host;", 1), set(&["host"]));
        assert_eq!(names("use super::super::ui::Ui;", 3), set(&[]));
    }

    #[test]
    fn depth_counts_the_modules_of_a_file_path() {
        assert_eq!(depth("ui.rs"), 1);
        assert_eq!(depth("ui/mod.rs"), 1);
        assert_eq!(depth("ui/frame.rs"), 2);
        assert_eq!(depth("ui/frame/mod.rs"), 2);
    }

    #[test]
    fn code_drops_comments_examples_and_the_trailing_tests() {
        let text = "/// crate::ui::Ui\nuse crate::map; // crate::host\n#[cfg(test)]\nmod tests {\n    use crate::ui;\n}\n";
        assert_eq!(names(&code(text), 1), set(&["map"]));
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
    fn a_tree_with_an_edge_up_and_a_join_out_of_order_fails() {
        let root = std::env::temp_dir().join(format!("xtask-domains-{}", std::process::id()));
        let src = root.join(SOURCES);
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
        write("lib.rs", "mod host; mod ui; mod script;");
        write("host.rs", "use crate::ui::Ui;\nuse crate::{script, ui};");
        write("ui.rs", "use crate::host::Host;");
        write("script.rs", "mod namespaces;");
        write(
            "script/namespaces.rs",
            "const D: &[&[N]] = &[\n    ui::NAMESPACES,\n    base::NAMESPACES,\n];",
        );
        write("script/namespaces/base.rs", "");
        write("script/namespaces/ui.rs", "");
        let problems = check(&root);
        std::fs::remove_dir_all(&root).unwrap();
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
        assert_eq!(problems.len(), 2, "{problems:?}");
    }

    #[test]
    fn the_map_of_core_holds_for_its_code() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        assert_eq!(check(&root), Vec::<String>::new());
    }
}
