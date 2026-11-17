use std::collections::{BTreeMap, BTreeSet};

use rustdoc_types::{
    Attribute, Crate, Enum, ExternalCrate, Generics, Id, Impl, Item, ItemEnum, ItemKind,
    ItemSummary, Module, Path, Struct, StructKind, Target, Type, Use, Variant, VariantKind,
    Visibility,
};

use super::walk::Surface;
use super::{built_on, render, stale};

fn item(id: u32, name: Option<&str>, visibility: Visibility, inner: ItemEnum) -> Item {
    Item {
        id: Id(id),
        crate_id: 0,
        name: name.map(str::to_owned),
        span: None,
        visibility,
        docs: None,
        links: std::iter::empty().collect(),
        attrs: Vec::new(),
        deprecation: None,
        stability: None,
        const_stability: None,
        inner,
    }
}

fn module(items: &[u32]) -> ItemEnum {
    ItemEnum::Module(Module {
        is_crate: false,
        items: items.iter().copied().map(Id).collect(),
        is_stripped: false,
    })
}

fn used(name: &str, id: Option<u32>, is_glob: bool) -> ItemEnum {
    ItemEnum::Use(Use {
        source: name.to_owned(),
        name: name.to_owned(),
        id: id.map(Id),
        is_glob,
    })
}

fn generics() -> Generics {
    Generics {
        params: Vec::new(),
        where_predicates: Vec::new(),
    }
}

fn function() -> ItemEnum {
    ItemEnum::Macro(String::new())
}

fn summary(crate_id: u32, path: &[&str], kind: ItemKind) -> ItemSummary {
    ItemSummary {
        crate_id,
        path: path.iter().map(|part| (*part).to_owned()).collect(),
        kind,
    }
}

fn krate(
    root: u32,
    items: Vec<Item>,
    paths: Vec<(u32, ItemSummary)>,
    external: &[(u32, &str)],
) -> Crate {
    Crate {
        root: Id(root),
        crate_version: None,
        includes_private: true,
        index: items.into_iter().map(|item| (item.id, item)).collect(),
        paths: paths.into_iter().map(|(id, path)| (Id(id), path)).collect(),
        external_crates: external
            .iter()
            .map(|(id, name)| {
                (
                    *id,
                    ExternalCrate {
                        name: (*name).to_owned(),
                        html_root_url: None,
                        path: std::path::PathBuf::new(),
                    },
                )
            })
            .collect(),
        target: Target {
            triple: String::new(),
            target_features: Vec::new(),
        },
        format_version: rustdoc_types::FORMAT_VERSION,
    }
}

/// `inner` keeps `Thing` in a private module and re-exports it; `outer`
/// re-exports from `inner` by the paths rustdoc gives the other crate's items.
fn crates() -> BTreeMap<String, Crate> {
    use Visibility::{Crate as Private, Default, Public};
    let inner = krate(
        0,
        vec![
            item(0, Some("inner"), Public, module(&[1, 2, 3, 4, 5, 6, 20])),
            item(1, Some("hidden_mod"), Private, module(&[10])),
            item(2, Some("Thing"), Public, used("Thing", Some(10), false)),
            item(3, Some("sub"), Public, module(&[30])),
            item(
                4,
                Some("Kind"),
                Public,
                ItemEnum::Enum(Enum {
                    generics: generics(),
                    has_stripped_variants: false,
                    variants: vec![Id(40), Id(41)],
                    impls: Vec::new(),
                }),
            ),
            item(5, Some("private_fn"), Private, function()),
            Item {
                attrs: vec![Attribute::Other("#[doc(hidden)]".to_owned())],
                ..item(6, Some("hidden_fn"), Public, function())
            },
            item(
                10,
                Some("Thing"),
                Public,
                ItemEnum::Struct(Struct {
                    kind: StructKind::Plain {
                        fields: vec![Id(11), Id(12)],
                        has_stripped_fields: false,
                    },
                    generics: generics(),
                    impls: vec![Id(20), Id(21)],
                }),
            ),
            item(11, Some("a"), Public, ItemEnum::StructField(Type::Infer)),
            item(12, Some("b"), Private, ItemEnum::StructField(Type::Infer)),
            item(20, None, Default, ItemEnum::Impl(block(None, &[22, 23]))),
            item(
                21,
                None,
                Default,
                ItemEnum::Impl(block(Some("Debug"), &[24])),
            ),
            item(22, Some("new"), Public, function()),
            item(23, Some("secret"), Private, function()),
            item(24, Some("fmt"), Default, function()),
            item(30, Some("f"), Public, function()),
            item(40, Some("One"), Default, variant()),
            item(41, Some("Two"), Default, variant()),
        ],
        vec![
            (3, summary(0, &["inner", "sub"], ItemKind::Module)),
            (4, summary(0, &["inner", "Kind"], ItemKind::Enum)),
            (
                10,
                summary(0, &["inner", "hidden_mod", "Thing"], ItemKind::Struct),
            ),
            (99, summary(0, &["inner", "Gone"], ItemKind::Function)),
        ],
        &[],
    );
    let outer = krate(
        0,
        vec![
            item(0, Some("outer"), Public, module(&[1, 2, 3, 4, 5, 6])),
            item(1, Some("sub"), Public, used("sub", Some(100), false)),
            item(2, Some("Thing"), Public, used("Thing", Some(101), false)),
            item(3, None, Public, used("Kind", Some(102), true)),
            item(4, Some("Gone"), Public, used("Gone", Some(103), false)),
            item(5, Some("Debug"), Public, used("Debug", Some(104), false)),
            item(6, Some("Two"), Public, function()),
        ],
        vec![
            (100, summary(1, &["inner", "sub"], ItemKind::Module)),
            (
                101,
                summary(1, &["inner", "hidden_mod", "Thing"], ItemKind::Struct),
            ),
            (102, summary(1, &["inner", "Kind"], ItemKind::Enum)),
            (103, summary(1, &["inner", "Gone"], ItemKind::Function)),
            (104, summary(2, &["core", "fmt", "Debug"], ItemKind::Trait)),
        ],
        &[(1, "inner"), (2, "core")],
    );
    [("inner".to_owned(), inner), ("outer".to_owned(), outer)].into()
}

fn block(trait_: Option<&str>, items: &[u32]) -> Impl {
    Impl {
        is_unsafe: false,
        generics: generics(),
        provided_trait_methods: Vec::new(),
        trait_: trait_.map(|name| Path {
            path: name.to_owned(),
            id: Id(500),
            args: None,
        }),
        for_: Type::ResolvedPath(Path {
            path: "Thing".to_owned(),
            id: Id(10),
            args: None,
        }),
        items: items.iter().copied().map(Id).collect(),
        is_negative: false,
        is_synthetic: false,
        blanket_impl: None,
    }
}

fn variant() -> ItemEnum {
    ItemEnum::Variant(Variant {
        kind: VariantKind::Plain,
        discriminant: None,
    })
}

fn set(paths: &[&str]) -> BTreeSet<String> {
    paths.iter().map(|path| (*path).to_owned()).collect()
}

#[test]
fn the_paths_follow_re_exports_across_crates_and_out_of_private_modules() {
    let crates = crates();
    assert_eq!(
        Surface::new(&crates).paths("outer").unwrap(),
        set(&[
            "Debug",
            "One",
            "Thing",
            "Thing::a",
            "Thing::fmt",
            "Thing::new",
            "Two",
            "sub",
            "sub::f",
        ])
    );
}

#[test]
fn the_paths_of_the_crate_itself_leave_private_items_and_members_out() {
    let crates = crates();
    assert_eq!(
        Surface::new(&crates).paths("inner").unwrap(),
        set(&[
            "Kind",
            "Kind::One",
            "Kind::Two",
            "Thing",
            "Thing::a",
            "Thing::fmt",
            "Thing::new",
            "sub",
            "sub::f",
        ])
    );
}

#[test]
fn a_crate_without_json_is_an_error() {
    let crates = crates();
    assert!(Surface::new(&crates).paths("missing").is_err());
}

#[test]
fn the_list_marks_the_paths_only_the_unstable_build_has() {
    let text = render(&set(&["a", "c"]), &set(&["a", "b", "c"]));
    let lines: Vec<&str> = text.lines().filter(|line| !line.starts_with('#')).collect();
    assert_eq!(lines, ["a", "b unstable", "c"]);
}

#[test]
fn a_stale_list_names_the_paths_it_lacks_and_those_gone() {
    assert!(stale("# old header\na\nb\n", "# new header\na\nb\n").is_empty());
    let problems = stale("a\nb unstable\n", "a\nb\nc\n");
    assert_eq!(problems.len(), 4, "{problems:?}");
    assert!(problems[0].contains("cargo xtask public-paths"));
    assert_eq!(&problems[1..], ["  + b", "  + c", "  - b unstable"]);
}

#[test]
fn the_documented_crates_are_the_required_workspace_dependencies() {
    let metadata = serde_json::json!({"packages": [
        {"name": "top", "dependencies": [
            {"name": "mid", "kind": null, "path": "x", "optional": false},
            {"name": "opt", "kind": null, "path": "x", "optional": true},
            {"name": "dev", "kind": "dev", "path": "x", "optional": false},
            {"name": "serde", "kind": null, "optional": false},
        ]},
        {"name": "mid", "dependencies": [
            {"name": "low", "kind": null, "path": "x", "optional": false},
        ]},
        {"name": "low", "dependencies": []},
        {"name": "opt", "dependencies": []},
        {"name": "dev", "dependencies": []},
    ]});
    assert_eq!(built_on(&metadata, "top"), ["low", "mid", "top"]);
}
