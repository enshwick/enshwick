use super::walk::{Named, ungated_paths};
use super::*;

const CORE_MANIFEST: &str = "crates/internal/core/Cargo.toml";

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
        loader: loader::Loader::default(),
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

pub(super) fn module(path: &str) -> Vec<String> {
    path.split("::").map(str::to_owned).collect()
}

/// A tree of files under a temporary root, removed however the test
/// ends.
pub(super) struct Tree(pub(super) std::path::PathBuf);

impl Tree {
    /// Writes `text` to `path` under the root, its folders made.
    pub(super) fn write(&self, path: &str, text: &str) {
        let path = self.0.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        let _gone = std::fs::remove_dir_all(&self.0);
    }
}

/// The top-level modules `code`, a file of module `module`, names.
fn names(
    code: &str,
    module: &[String],
    roots: &BTreeMap<String, String>,
) -> Result<BTreeSet<String>, String> {
    let file = syn::parse_file(code).map_err(|e| e.to_string())?;
    Ok(tops(paths(&file, module, &[]).found, roots))
}

fn named(code: &str, module: &str, externs: &[&str]) -> Result<Named, String> {
    let file = syn::parse_file(code).map_err(|e| e.to_string())?;
    Ok(paths(&file, &self::module(module), externs))
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
fn a_self_path_resolves_in_its_module_and_a_super_chain_past_the_root_is_dropped()
-> Result<(), String> {
    let found = named("use self::part::P;", "world", &[])?.found;
    assert_eq!(found, [module("world::part::P")].into());
    let found = named("use super::super::x;", "world", &[])?.found;
    assert!(found.is_empty(), "{found:?}");
    Ok(())
}

#[test]
fn paths_keep_a_named_crate_s_paths_and_the_relative_ones_apart() -> Result<(), String> {
    let code = "use enshwick_core::ui::Frame;\nuse std::fmt;\nfn f() { other::x(); enshwick_core::chat::send(); ::enshwick_core::map::open(); ::std::mem::drop(1); }";
    let named = named(code, "test::world", &[fake::CORE_CRATE])?;
    assert!(named.found.is_empty(), "{:?}", named.found);
    let joined = |paths: &BTreeSet<Vec<String>>| -> Vec<String> {
        paths.iter().map(|p| p.join("::")).collect()
    };
    assert_eq!(
        joined(&named.external),
        [
            "enshwick_core::chat::send",
            "enshwick_core::map::open",
            "enshwick_core::ui::Frame"
        ]
    );
    assert!(
        named.relative.contains(&module("other::x")),
        "{:?}",
        named.relative
    );
    Ok(())
}

#[test]
fn an_ungated_walk_leaves_out_only_items_impl_items_and_trait_items_under_a_feature()
-> Result<(), String> {
    let code = "#[cfg(feature = \"x\")]\nuse a::A;\n#[cfg(not(feature = \"x\"))]\nuse b::B;\nimpl T {\n    #[cfg(feature = \"x\")] const C: u8 = c::C;\n    #[cfg(feature = \"x\")] fn f() { d::g(); }\n    #[cfg(feature = \"x\")] type Y = e::Y;\n    #[cfg(feature = \"x\")] m!(f::x);\n    #[other(feature = \"x\")] fn o() { g::g(); }\n    fn h() { h::g(); }\n}\ntrait U {\n    #[cfg(feature = \"x\")] const K: u8 = i::K;\n    #[cfg(feature = \"x\")] fn m() { j::g(); }\n    #[cfg(feature = \"x\")] type Z = k::Z;\n    #[cfg(feature = \"x\")] n!(l::x);\n    fn p() { m::g(); }\n}";
    let externs = [
        "a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m",
    ];
    let file = syn::parse_file(code).map_err(|e| e.to_string())?;
    let firsts = |named: Named| -> BTreeSet<String> {
        named
            .external
            .into_iter()
            .filter_map(|path| path.into_iter().next())
            .collect()
    };
    assert_eq!(
        firsts(ungated_paths(&file, &[], &externs)),
        set(&["b", "g", "h", "m"])
    );
    assert_eq!(firsts(paths(&file, &[], &externs)), set(&externs));
    Ok(())
}

#[test]
fn use_leaves_bind_self_and_renames_and_keep_globs_apart() -> Result<(), syn::Error> {
    let item: syn::ItemUse = syn::parse_str("use a::{self as b, c::{self}, d as e, f::*};")?;
    let leaves = use_leaves(&item.tree);
    let leaf = |name: Option<&str>, path: &str| (name.map(str::to_owned), module(path));
    assert_eq!(
        leaves,
        [
            leaf(Some("b"), "a"),
            leaf(Some("c"), "a::c"),
            leaf(Some("e"), "a::d"),
            leaf(None, "a::f")
        ]
    );
    Ok(())
}

#[test]
fn names_skips_comments_strings_and_test_items_but_reads_code_after_them() -> Result<(), String> {
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
        "pub use world_error::WorldError;\npub use load::{Load as L, Mod};\nuse ui::Hidden;\npub use enshwick_content as content;\npub use crate::map::{self as atlas};",
    )?;
    assert_eq!(
        roots.get("content").map(String::as_str),
        Some("enshwick_content")
    );
    assert_eq!(roots.get("atlas").map(String::as_str), Some("map"));
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
    let down = edges(&[("ui", "map")]);
    let problems = judge(&map(&["ui -> map"]), &set(&["host", "map", "ui"]), &down);
    assert!(
        problems.len() == 1 && problems[0].contains("ui -> map does not go up"),
        "{problems:?}"
    );
}

#[test]
fn a_tree_with_an_edge_up_a_stray_namespace_and_a_join_out_of_order_fails() {
    let root = std::env::temp_dir().join(format!("xtask-domains-{}", std::process::id()));
    let _gone = std::fs::remove_dir_all(&root);
    let tree = Tree(root.clone());
    let write = |path: &str, text: &str| tree.write(&format!("{SOURCES}/{path}"), text);
    tree.write(
        MAP,
        "order = [\"base\", \"ui\"]\njoins = [\"lib.rs\", \"script/namespaces\"]\nupward = []\n[modules]\nbase = [\"host\", \"script\"]\nui = [\"ui\"]\n",
    );
    write("lib.rs", "mod host; mod ui; mod script;\npub use ui::Ui;");
    write("host.rs", "use crate::Ui;\nuse crate::script;");
    write("ui.rs", "use crate::host::Host;");
    write(
        "script/namespaces.rs",
        "const D: &[&[N]] = &[\n    ui::NAMESPACES,\n    base::NAMESPACES,\n];",
    );
    write(
        "script/namespaces/base.rs",
        "const N: &[u8] = &[crate::ui::X];",
    );
    // A list's own `self::` paths stay inside the lists; a path to the
    // base runtime is another domain's.
    write(
        "script/namespaces/ui.rs",
        "use self::inner::X;\nconst S: u8 = crate::script::Y;",
    );
    write("script/checks.rs", "use crate::ui::Ui;");
    write("script.rs", "mod namespaces;\n#[cfg(test)]\nmod checks;");
    tree.write(&format!("{}/{}.rs", fake::SOURCES, fake::ROOT), "");
    tree.write(CORE_MANIFEST, "[package]\nname = \"enshwick-core\"\n");
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
    assert!(
        problems
            .iter()
            .any(|p| p.contains("ui.rs names script of base")),
        "{problems:?}"
    );
    assert_eq!(problems.len(), 4, "{problems:?}");
}

#[test]
fn a_load_method_calling_on_self_one_of_a_domain_above_is_an_edge_up() {
    let root = std::env::temp_dir().join(format!("xtask-domains-calls-{}", std::process::id()));
    let _gone = std::fs::remove_dir_all(&root);
    let tree = Tree(root.clone());
    let write = |path: &str, text: &str| tree.write(&format!("{SOURCES}/{path}"), text);
    tree.write(
        MAP,
        "order = [\"base\", \"ui\"]\njoins = [\"lib.rs\"]\nupward = []\n[modules]\nbase = [\"altars\", \"map\"]\nui = [\"ui\"]\n",
    );
    write("lib.rs", "mod altars; mod map; mod ui;");
    write("ui.rs", "impl Load<'_> { pub fn on_frame(&mut self) {} }");
    write(
        "altars.rs",
        "impl Load<'_> { pub fn limit(&mut self) { let _ = || self.on_frame(); } }",
    );
    // Another type's `self`, and a `Load` block only tests build.
    write(
        "map.rs",
        "impl Other { fn f(&self) { self.on_frame(); } }\n#[cfg(test)]\nimpl Load<'_> { fn t(&mut self) { self.on_frame(); } }",
    );
    tree.write(&format!("{}/{}.rs", fake::SOURCES, fake::ROOT), "");
    tree.write(CORE_MANIFEST, "[package]\nname = \"enshwick-core\"\n");
    let problems = check(&root);
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(
        problems[0].starts_with("altars -> ui goes up"),
        "{problems:?}"
    );
}

#[test]
fn the_map_of_core_holds_for_its_code() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    assert_eq!(check(&root), Vec::<String>::new());
}
