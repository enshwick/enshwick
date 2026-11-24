//! Which features a local check lints alone.

use std::collections::BTreeMap;

use super::*;
use features::Alone;

fn packages() -> [Package; 2] {
    let mut content = package("enshwick-content", "crates/api/content", false);
    content.features = ["fake", "typed", "unstable"].map(str::to_owned).into();
    let mut world = package("enshwick-world", "crates/api/world", false);
    world.features = vec!["fake".to_owned()];
    [content, world]
}

fn files(paths: &[&str]) -> Vec<String> {
    paths.iter().map(|path| (*path).to_owned()).collect()
}

/// The texts of the source files the tests change.
fn read(path: &str) -> Option<String> {
    match path {
        "crates/api/content/src/fake.rs" => Some(
            "#[cfg(feature = \"fake\")] pub fn f() {}\n\
             #[cfg_attr(not(feature = \"unstable\"), expect(dead_code))] fn g() {}\n\
             #[cfg(test)] mod tests { #[cfg(feature = \"typed\")] fn t() {} }"
                .to_owned(),
        ),
        "crates/api/content/src/plain.rs" => Some("pub fn plain() {}".to_owned()),
        "crates/api/content/src/broken.rs" => Some("fn f() { \"open".to_owned()),
        _ => None,
    }
}

fn touched(entries: &[(&str, Option<&[&str]>)]) -> Alone {
    Alone::Touched(
        entries
            .iter()
            .map(|(name, features)| {
                let features =
                    features.map(|names| names.iter().map(|n| (*n).to_owned()).collect());
                ((*name).to_owned(), features)
            })
            .collect::<BTreeMap<_, _>>(),
    )
}

#[test]
fn a_changed_source_file_reaches_only_the_features_its_cfg_predicates_name_outside_test_code() {
    let alone = Alone::of(
        &files(&["crates/api/content/src/fake.rs"]),
        &packages(),
        read,
    );
    assert_eq!(
        alone,
        touched(&[("enshwick-content", Some(&["fake", "unstable"]))])
    );
}

#[test]
fn a_source_file_without_a_feature_gate_reaches_no_feature_and_a_test_or_a_file_not_rust_reaches_nothing()
 {
    let changed = files(&[
        "crates/api/content/src/plain.rs",
        "crates/api/content/src/thing/tests.rs",
        "crates/api/content/tests/main.rs",
        "crates/api/content/examples/demo.rs",
        "crates/api/content/CHANGELOG.md",
        "crates/api/content/changes/a.md",
        "crates/api/content/src/table.toml",
        "crates/api/content/src/snapshots/a__b.snap",
        "crates/api/world/include/world.h",
        "crates/api/world/README.md",
        "book/src/intro.md",
    ]);
    assert_eq!(
        Alone::of(&changed, &packages(), read),
        touched(&[("enshwick-content", Some(&[]))])
    );
}

#[test]
fn a_manifest_a_build_script_rust_outside_src_or_a_source_file_gone_or_unlexed_reaches_every_feature()
 {
    for file in [
        "crates/api/content/Cargo.toml",
        "crates/api/content/build.rs",
        "crates/api/content/build/tables.rs",
        "crates/api/content/src/gone.rs",
        "crates/api/content/src/broken.rs",
    ] {
        let changed = files(&["crates/api/content/src/fake.rs", file]);
        assert_eq!(
            Alone::of(&changed, &packages(), read),
            touched(&[("enshwick-content", None)]),
            "{file}"
        );
    }
}

#[test]
fn a_change_to_the_shared_manifest_or_lock_reaches_every_feature() {
    for file in ["Cargo.toml", "Cargo.lock"] {
        let changed = files(&["crates/api/content/src/plain.rs", file]);
        assert_eq!(
            Alone::of(&changed, &packages(), read),
            Alone::Every,
            "{file}"
        );
    }
}

#[test]
fn the_feature_lints_run_each_package_without_features_and_alone_only_the_features_reached() {
    let packages = packages();
    let scope = Scope::Some {
        code: ["enshwick-content", "enshwick-world"]
            .map(str::to_owned)
            .into(),
        docs: BTreeSet::new(),
        tested: BTreeSet::new(),
        built: BTreeSet::new(),
        wide: false,
    };
    let alone = touched(&[("enshwick-content", Some(&["unstable"]))]);
    let lines: Vec<String> = features::steps(&scope, &packages, |_| true, &["clippy"], &alone)
        .iter()
        .map(Step::line)
        .collect();
    let content = "cargo clippy --package enshwick-content --no-default-features";
    let world = "cargo clippy --package enshwick-world --no-default-features";
    assert_eq!(
        lines,
        [
            format!("{content} -- -D warnings"),
            format!("{content} --features unstable -- -D warnings"),
            format!("{world} -- -D warnings"),
        ]
    );
    let every = touched(&[("enshwick-content", None)]);
    assert_eq!(
        features::steps(&scope, &packages, |_| true, &["clippy"], &every).len(),
        1 + 3 + 1
    );
}

#[test]
fn the_features_cfg_predicates_name_are_read_from_attributes_and_macros() {
    let gated =
        |text: &str| features::gated_features(text).map(|set| set.into_iter().collect::<Vec<_>>());
    assert_eq!(
        gated("fn f() { if cfg!(all(feature = \"a\", not(feature = \"b\"))) {} }"),
        Some(vec!["a".to_owned(), "b".to_owned()])
    );
    assert_eq!(
        gated("#[cfg_attr(docsrs, doc(cfg(feature = \"c\")))] pub fn f() {}"),
        Some(vec!["c".to_owned()])
    );
    assert_eq!(
        gated("// #[cfg(feature = \"d\")]\nconst S: &str = \"feature = e\";"),
        Some(vec![])
    );
    assert_eq!(gated("fn f() { \"open"), None);
}

#[test]
fn each_checked_package_is_linted_without_features_and_with_each_alone() {
    let mut content = package("enshwick-content", "crates/api/content", false);
    content.features = vec!["fake".to_owned(), "unstable".to_owned()];
    let mut stub = package("enshwick-loader-stub", "crates/runtime/stub", true);
    stub.features = vec!["trace".to_owned()];
    let mut untouched = package("enshwick-world", "crates/api/world", false);
    untouched.features = vec!["fake".to_owned()];
    let packages = [
        content,
        stub,
        untouched,
        package("enshwick-abi", "crates/abi", false),
    ];
    let scope = Scope::Some {
        code: ["enshwick-content", "enshwick-loader-stub", "enshwick-abi"]
            .map(str::to_owned)
            .into(),
        docs: BTreeSet::new(),
        tested: BTreeSet::new(),
        built: BTreeSet::new(),
        wide: false,
    };
    let host_ok = |p: &Package| p.name != "enshwick-loader-stub";
    let lines: Vec<String> =
        features::steps(&scope, &packages, host_ok, &["clippy"], &Alone::Every)
            .iter()
            .map(Step::line)
            .collect();
    let clippy = "cargo clippy --package enshwick-content --no-default-features";
    assert_eq!(
        lines,
        [
            format!("{clippy} -- -D warnings"),
            format!("{clippy} --features fake -- -D warnings"),
            format!("{clippy} --features unstable -- -D warnings"),
        ]
    );
    assert_eq!(
        features::steps(&Scope::All, &packages, |_| true, &["clippy"], &Alone::Every).len(),
        3 + 2 + 2
    );
}

#[test]
fn the_windows_lint_lints_each_feature_alone_of_the_checked_packages_with_windows_code() {
    let mut stub = package("enshwick-loader-stub", "crates/runtime/stub", true);
    stub.features = vec!["trace".to_owned()];
    let mut portable = package("enshwick-values", "crates/internal/values", false);
    portable.features = vec!["serde".to_owned()];
    let mut untouched = package("enshwick-world", "crates/api/world", false);
    untouched.features = vec!["fake".to_owned()];
    let packages = [stub, portable, untouched];
    let scope = Scope::Some {
        code: ["enshwick-loader-stub", "enshwick-values"]
            .map(str::to_owned)
            .into(),
        docs: BTreeSet::new(),
        tested: BTreeSet::new(),
        built: BTreeSet::new(),
        wide: false,
    };
    let windows_code = |p: &Package| p.name != "enshwick-values";
    let lines = |parts: &[Part]| -> Vec<String> {
        windows_lint::steps(&scope, &packages, parts, windows_code, &Alone::Every)
            .iter()
            .map(Step::line)
            .collect()
    };
    let xwin = format!("cargo xwin clippy --target {WINDOWS_TARGET}");
    let stub = format!("{xwin} --package enshwick-loader-stub --no-default-features");
    let workspace =
        format!("{xwin} --workspace --features {TEST_FEATURE} --all-targets -- -D warnings");
    let alone = [
        format!("{stub} -- -D warnings"),
        format!("{stub} --features trace -- -D warnings"),
    ];
    assert_eq!(
        lines(&[Part::WindowsLint]),
        std::slice::from_ref(&workspace)
    );
    assert_eq!(lines(&[Part::WindowsFeatures]), alone);
    assert_eq!(
        lines(&Part::every(false)),
        [[workspace].as_slice(), &alone].concat()
    );
    assert!(lines(&[Part::Host, Part::Features]).is_empty());
}
