use std::collections::BTreeSet;

use clap::ValueEnum as _;

use super::*;

#[test]
fn the_features_docs_stable_trybuild_and_semver_parts_parse_and_run_when_no_part_is_named() {
    assert_eq!(Part::from_str("features", false), Ok(Part::Features));
    assert_eq!(Part::from_str("docs", false), Ok(Part::Docs));
    assert_eq!(Part::from_str("stable-api", false), Ok(Part::StableApi));
    assert_eq!(
        Part::from_str("windows-tests", false),
        Ok(Part::WindowsTests)
    );
    assert_eq!(Part::from_str("trybuild", false), Ok(Part::Trybuild));
    assert_eq!(Part::from_str("semver", false), Ok(Part::Semver));
    for part in [
        Part::Features,
        Part::Docs,
        Part::StableApi,
        Part::Trybuild,
        Part::Semver,
    ] {
        assert!(Part::every(false).contains(&part), "{part:?}");
        assert!(Part::every(true).contains(&part), "{part:?}");
    }
    assert!(Part::from_str("doc", false).is_err());
}

/// The step lines `parts` plan for a change to the packages `code`.
fn planned_for(code: &[&str], parts: &[Part]) -> Vec<String> {
    let scope = Scope::Some {
        code: code.iter().map(|name| (*name).to_owned()).collect(),
        docs: BTreeSet::new(),
        tested: BTreeSet::new(),
        built: BTreeSet::new(),
        wide: false,
    };
    let Scope::Some { code, .. } = &scope else {
        unreachable!()
    };
    let filter = test_filter(code.iter());
    host_steps(
        &scope,
        (&[], &[]),
        (true, true, false),
        Some(&filter),
        &[],
        false,
        parts,
    )
    .iter()
    .map(Step::line)
    .collect()
}

#[test]
fn the_trybuild_part_runs_the_test_only_when_the_api_crate_s_tests_are_in_scope() {
    let trybuild = format!(
        "cargo nextest run --package enshwick --test main --features {TRYBUILD_FEATURES} --no-tests=fail -E {TRYBUILD}"
    );
    assert_eq!(
        planned_for(&["enshwick-core", "enshwick"], &[Part::Trybuild]),
        [trybuild.as_str()]
    );
    assert!(planned_for(&["cargo-enshwick"], &[Part::Trybuild]).is_empty());
    assert_eq!(planned(&[Part::Trybuild]), [trybuild.as_str()]);
}

#[test]
fn the_host_part_never_runs_the_trybuild_test_whatever_parts_run_beside_it() {
    let nextest = |steps: Vec<String>| {
        steps
            .into_iter()
            .find(|step| step.starts_with("cargo nextest run --no-tests=pass"))
            .unwrap()
    };
    let in_scope = format!("-E (package(enshwick)) & not ({TRYBUILD})");
    for parts in [&[Part::Host][..], &[Part::Host, Part::Trybuild]] {
        let step = nextest(planned_for(&["enshwick"], parts));
        assert!(step.ends_with(&in_scope), "{parts:?}: {step}");
    }
    let everything = format!("-E not ({TRYBUILD})");
    assert!(nextest(planned(&Part::every(false))).ends_with(&everything));
    assert!(nextest(planned(&[Part::Host])).ends_with(&everything));
}

#[test]
fn the_host_tests_take_their_filter_after_the_workspace_arguments() {
    let workspace = ["--workspace".to_owned()];
    let steps = host_steps(
        &Scope::All,
        (&workspace, &[]),
        (true, true, true),
        None,
        &[],
        false,
        &[Part::Host],
    );
    let nextest = steps
        .iter()
        .map(Step::line)
        .find(|step| step.starts_with("cargo nextest run"))
        .unwrap();
    assert!(nextest.contains("--workspace -E"), "{nextest}");
}

#[test]
fn only_a_wide_check_that_checks_the_stable_surface_says_it_left_its_examples_to_ci() {
    let set = |names: &[&str]| names.iter().map(|name| (*name).to_owned()).collect();
    let scope =
        |code: &[&str], docs: &[&str], tested: &[&str], built: &[&str], wide: bool| Scope::Some {
            code: set(code),
            docs: set(docs),
            tested: set(tested),
            built: set(built),
            wide,
        };
    let parts = [Part::Host, Part::StableApi];
    for due in [
        scope(&[FACADE], &[], &[], &[], true),
        scope(&["enshwick-abi"], &[FACADE], &[], &[], true),
        scope(&["enshwick-abi"], &[], &[FACADE], &[], true),
    ] {
        assert!(stable_examples_left_to_ci(&due, &parts), "{due:?}");
    }
    for not_due in [
        scope(&["enshwick-abi"], &[], &[], &[FACADE], true),
        scope(&["enshwick-abi"], &[], &[FACADE], &[], false),
    ] {
        assert!(!stable_examples_left_to_ci(&not_due, &parts), "{not_due:?}");
    }
    assert!(!stable_examples_left_to_ci(
        &scope(&[FACADE], &[], &[], &[], true),
        &[Part::Host]
    ));
    assert!(!stable_examples_left_to_ci(
        &scope(&[FACADE], &[], &[], &[], false),
        &parts
    ));
}

#[test]
fn a_wide_change_lints_and_documents_the_stable_surface_but_leaves_its_examples() {
    let scope = Scope::Some {
        code: ["enshwick-abi".to_owned()].into(),
        docs: BTreeSet::new(),
        tested: [FACADE.to_owned()].into(),
        built: BTreeSet::new(),
        wide: true,
    };
    let stable = stable();
    let steps = host_steps(
        &scope,
        (&["--workspace".to_owned()], &stable),
        (true, true, false),
        Some(""),
        &[],
        false,
        &[Part::StableApi],
    );
    let lines: Vec<String> = steps.iter().map(Step::line).collect();
    let stable_line = stable.join(" ");
    assert!(
        lines.contains(&format!(
            "cargo clippy --all-targets {stable_line} -- -D warnings"
        )),
        "{lines:?}"
    );
    assert!(runs_stable_rustdoc(&steps, &stable), "{lines:?}");
    assert!(
        !lines
            .iter()
            .any(|line| line.starts_with("cargo test --doc")),
        "{lines:?}"
    );
}

#[test]
fn a_wide_change_that_reaches_only_the_facade_s_documents_does_not_lint_its_stable_surface() {
    let scope = Scope::Some {
        code: ["enshwick-abi".to_owned()].into(),
        docs: [FACADE.to_owned()].into(),
        tested: BTreeSet::new(),
        built: BTreeSet::new(),
        wide: true,
    };
    let stable = stable();
    let steps = host_steps(
        &scope,
        (&["--workspace".to_owned()], &stable),
        (true, true, false),
        Some(""),
        &[],
        false,
        &[Part::StableApi],
    );
    let lines: Vec<String> = steps.iter().map(Step::line).collect();
    assert!(
        !lines.contains(&format!(
            "cargo clippy --all-targets {} -- -D warnings",
            stable.join(" ")
        )),
        "{lines:?}"
    );
    assert!(runs_stable_rustdoc(&steps, &stable), "{lines:?}");
}

/// CI starts the trybuild and stable surface jobs only for the parts this
/// lists, so a part listed here must have a step and one left out none.
#[test]
fn only_a_change_that_reaches_the_facade_lists_the_trybuild_and_stable_surface_parts() {
    let scope = |code: &[&str], docs: &[&str], wide: bool| Scope::Some {
        code: code.iter().map(|name| (*name).to_owned()).collect(),
        docs: docs.iter().map(|name| (*name).to_owned()).collect(),
        tested: BTreeSet::new(),
        built: BTreeSet::new(),
        wide,
    };
    let due = |scope: &Scope| -> Vec<Part> {
        [Part::Trybuild, Part::StableApi]
            .into_iter()
            .filter(|part| part.due(scope, true))
            .collect()
    };
    assert_eq!(due(&scope(&["xtask"], &[], false)), []);
    assert_eq!(
        due(&scope(&[FACADE], &[], false)),
        [Part::Trybuild, Part::StableApi]
    );
    assert_eq!(due(&scope(&[], &[FACADE], false)), [Part::StableApi]);
    assert_eq!(due(&scope(&[FACADE], &[], true)), [Part::StableApi]);
    assert_eq!(due(&Scope::All), [Part::Trybuild, Part::StableApi]);
    for (part, planned) in [
        (Part::Trybuild, planned_for(&["xtask"], &[Part::Trybuild])),
        (Part::StableApi, planned_for(&["xtask"], &[Part::StableApi])),
    ] {
        assert!(!part.due(&scope(&["xtask"], &[], false), true));
        assert!(planned.is_empty(), "{part:?} plans {planned:?}");
    }
    assert!(Part::Host.due(&scope(&["xtask"], &[], false), true));
}

/// CI starts the Windows feature lint job only where this lists it, so a
/// change that checks no package with features and Windows code takes no
/// runner for it.
#[test]
fn only_a_change_that_checks_a_package_with_features_and_windows_code_lists_the_windows_features_part()
 {
    let mut loader = package("enshwick-loader", "crates/runtime/loader", true);
    loader.features = vec!["test".to_owned()];
    let mut values = package("enshwick-values", "crates/internal/values", false);
    values.features = vec!["serde".to_owned()];
    let plain = package("enshwick-abi", "crates/abi", true);
    let packages = [loader, values, plain];
    let windows_code = |p: &Package| p.name != "enshwick-values";
    let listed = |scope: &Scope| {
        let due = windows_features_due(scope, &packages, windows_code);
        let planned = windows_lint::steps(
            scope,
            &packages,
            &[Part::WindowsFeatures],
            windows_code,
            &features::Alone::Every,
        );
        assert_eq!(due, !planned.is_empty(), "the listing and the plan differ");
        due_parts(&[Part::WindowsFeatures], scope, due)
    };
    let some = |code: &[&str]| Scope::Some {
        code: code.iter().map(|name| (*name).to_owned()).collect(),
        docs: BTreeSet::new(),
        tested: BTreeSet::new(),
        built: BTreeSet::new(),
        wide: false,
    };
    assert!(listed(&some(&["enshwick-values"])).is_empty());
    assert!(listed(&some(&["enshwick-abi"])).is_empty());
    assert!(listed(&some(&[])).is_empty());
    assert_eq!(
        listed(&some(&["enshwick-values", "enshwick-loader"])),
        ["windows-features"]
    );
    assert_eq!(listed(&Scope::All), ["windows-features"]);
}

#[test]
fn a_stable_api_check_says_when_the_change_does_not_reach_the_stable_surface() {
    let scope = |code: &[&str], docs: &[&str]| Scope::Some {
        code: code.iter().map(|name| (*name).to_owned()).collect(),
        docs: docs.iter().map(|name| (*name).to_owned()).collect(),
        tested: BTreeSet::new(),
        built: BTreeSet::new(),
        wide: false,
    };
    let parts = [Part::StableApi];
    assert!(stable_api_out_of_scope(&scope(&["xtask"], &[]), &parts));
    assert!(!stable_api_out_of_scope(&scope(&[FACADE], &[]), &parts));
    assert!(!stable_api_out_of_scope(&scope(&[], &[FACADE]), &parts));
    assert!(!stable_api_out_of_scope(&Scope::All, &parts));
    assert!(!stable_api_out_of_scope(
        &scope(&["xtask"], &[]),
        &[Part::Host]
    ));
}

#[test]
fn the_semver_part_checks_each_package_it_is_given_and_none_without() {
    assert!(semver_step(&[]).is_none());
    let step = semver_step(&["enshwick".to_owned(), "enshwick-core".to_owned()]).unwrap();
    assert_eq!(
        step.line(),
        "cargo semver-checks --package enshwick --package enshwick-core"
    );
}

fn planned(parts: &[Part]) -> Vec<String> {
    host_steps(
        &Scope::All,
        (&[], &stable()),
        (true, true, true),
        None,
        &[],
        true,
        parts,
    )
    .iter()
    .map(Step::line)
    .collect()
}

#[test]
fn the_host_docs_and_stable_parts_split_the_steps_a_check_of_every_part_runs_in_order() {
    let host = planned(&[Part::Host, Part::Trybuild]);
    let docs = planned(&[Part::Docs]);
    let stable_part = planned(&[Part::StableApi]);
    let args = stable();
    let starts = |steps: &[String], with: &str| steps.iter().any(|s| s.starts_with(with));
    assert!(starts(&host, "cargo nextest") && starts(&host, "cargo deny"));
    assert!(!starts(&host, "cargo test --doc") && !starts(&host, "cargo doc"));
    assert!(starts(&docs, "cargo test --doc") && starts(&docs, "cargo doc"));
    assert!(!starts(&docs, "cargo nextest") && !starts(&docs, "cargo fmt"));
    let stable_line = args.join(" ");
    assert!(!docs.iter().any(|s| s.contains(&stable_line)), "{docs:?}");
    assert!(starts(&stable_part, "cargo test --doc") && starts(&stable_part, "cargo doc"));
    let every = planned(&Part::every(false));
    // Each part's steps come in the full run's order: a step that runs
    // twice, such as the link check after each docs build, is found after
    // the part's step before it.
    let in_order = |steps: &[String]| {
        let mut from = 0;
        steps.iter().all(|step| {
            let found = (from..every.len()).find(|at| every[*at] == *step);
            found.inspect(|at| from = at + 1).is_some()
        })
    };
    assert_eq!(every.len(), host.len() + docs.len() + stable_part.len());
    assert!(in_order(&host) && in_order(&docs) && in_order(&stable_part));
}

#[test]
fn a_change_to_the_api_docs_alone_plans_the_stable_steps_in_their_part() {
    let docs_of = |name: &str| Scope::Some {
        code: BTreeSet::new(),
        docs: [name.to_owned()].into(),
        tested: BTreeSet::new(),
        built: BTreeSet::new(),
        wide: false,
    };
    let args = stable();
    let plans_stable = |scope: &Scope| {
        host_steps(
            scope,
            (&[], &args),
            (false, false, true),
            Some(""),
            &[],
            false,
            &[Part::StableApi],
        )
        .iter()
        .any(|step| step.args.ends_with(&args))
    };
    assert!(plans_stable(&docs_of("enshwick")));
    assert!(!plans_stable(&docs_of("enshwick-luau")));
}

#[test]
fn the_doctests_run_only_the_examples_of_the_packages_they_are_given() {
    let packages = packages().unwrap();
    let names = ["enshwick-core".to_owned(), "xtask".to_owned()];
    let filters = doctest_filters(names.iter(), &packages);
    assert_eq!(
        filters,
        [
            "crates/internal/core/",
            "crates\\internal\\core\\",
            "tools/xtask/",
            "tools\\xtask\\",
        ]
    );
    let scope = Scope::Some {
        code: names.into(),
        docs: BTreeSet::new(),
        tested: BTreeSet::new(),
        built: BTreeSet::new(),
        wide: false,
    };
    let steps = host_steps(
        &scope,
        (&[], &[]),
        (true, true, true),
        Some(""),
        &filters,
        false,
        &[Part::Docs],
    );
    let doctest = steps
        .iter()
        .find(|s| s.args.starts_with(&["test".to_owned(), "--doc".to_owned()]));
    assert!(
        doctest
            .unwrap()
            .line()
            .ends_with(&format!("-- {}", filters.join(" ")))
    );
    // The book's chapters are the facade's examples, under its directory.
    let book = "crates/enshwick/src/../../book/src/hooks.md - Hooks (line 12)";
    let facade = doctest_filters(["enshwick".to_owned()].iter(), &packages);
    assert!(facade.iter().any(|filter| book.contains(filter.as_str())));
    assert!(
        !facade
            .iter()
            .any(|filter| "crates/enshwick-gamedata/src/lib.rs".contains(filter.as_str()))
    );
}

#[test]
fn a_package_only_built_is_linted_without_tests_examples_or_rustdoc() {
    let scope = Scope::Some {
        code: BTreeSet::new(),
        docs: BTreeSet::new(),
        tested: BTreeSet::new(),
        built: ["enshwick-core".to_owned()].into(),
        wide: false,
    };
    let steps: Vec<String> = host_steps(
        &scope,
        (&[], &[]),
        (true, false, false),
        Some(""),
        &[],
        false,
        &Part::every(false),
    )
    .iter()
    .map(Step::line)
    .collect();
    assert!(
        steps.iter().any(|s| s.starts_with("cargo clippy")),
        "{steps:?}"
    );
    for absent in ["cargo nextest", "cargo test --doc", "cargo doc"] {
        assert!(
            !steps.iter().any(|s| s.starts_with(absent)),
            "{absent}: {steps:?}"
        );
    }
}

fn runs_stable_rustdoc(steps: &[Step], stable: &[String]) -> bool {
    steps.iter().any(|step| {
        step.args
            .starts_with(&["doc".to_owned(), "--no-deps".to_owned()])
            && step.args.ends_with(stable)
            && step
                .env
                .iter()
                .any(|(name, value)| *name == "RUSTDOCFLAGS" && value == "-D warnings")
    })
}

/// The stable API's arguments for this workspace.
fn stable() -> Vec<String> {
    stable_api(&packages().unwrap())
}

#[test]
fn the_stable_api_is_the_facade_and_each_internal_crate_whose_examples_name_it() {
    let stable = stable();
    let named = |name: &str| stable.iter().any(|arg| arg == name);
    for name in [
        "enshwick",
        "enshwick-core",
        "enshwick-base",
        "enshwick-content",
        "enshwick-world",
        "enshwick-fake",
        "enshwick-values",
        "enshwick-abi",
    ] {
        assert!(named(name), "{name}");
    }
    // Typed's examples need the facade's `typed`; the macros and the
    // runtimes have no stable surface of their own to check.
    for name in [
        "enshwick-typed",
        "enshwick-macros",
        "enshwick-luau",
        "xtask",
    ] {
        assert!(!named(name), "{name}");
    }
    assert!(stable.ends_with(&["--features".to_owned(), "enshwick/test".to_owned()]));
}

#[test]
fn a_package_only_built_is_reached_but_not_touched() {
    let scope = Scope::Some {
        code: ["enshwick-abi".to_owned()].into(),
        docs: BTreeSet::new(),
        tested: BTreeSet::new(),
        built: ["enshwick".to_owned()].into(),
        wide: false,
    };
    assert!(reaches(&scope, "enshwick") && !touches(&scope, "enshwick"));
    assert!(reaches(&scope, "enshwick-abi") && touches(&scope, "enshwick-abi"));
    assert!(!reaches(&scope, "xtask"));
    assert!(reaches(&Scope::All, "xtask"));
}

#[test]
fn a_package_whose_tests_alone_run_gets_no_examples_trybuild_or_stable_surface_steps() {
    let packages = [
        package("enshwick-world", "crates/internal/world", false),
        package("enshwick", "crates/enshwick", false),
        package("enshwick-loader", "crates/runtime/loader", true),
    ];
    let scope = Scope::Some {
        code: ["enshwick-world".to_owned()].into(),
        docs: BTreeSet::new(),
        tested: ["enshwick".to_owned(), "enshwick-loader".to_owned()].into(),
        built: BTreeSet::new(),
        wide: true,
    };
    assert!(reaches(&scope, "enshwick") && !touches(&scope, "enshwick"));
    let (flags, filter, doctests) = plan(&scope, &packages, |name| name != "enshwick-loader");
    assert_eq!(
        filter.as_deref(),
        Some("package(enshwick-world) | package(enshwick)")
    );
    assert_eq!(
        doctests,
        ["crates/internal/world/", "crates\\internal\\world\\"]
    );
    let steps: Vec<String> = host_steps(
        &scope,
        (&[], &[]),
        flags,
        filter.as_deref(),
        &doctests,
        false,
        &Part::every(false),
    )
    .iter()
    .map(Step::line)
    .collect();
    assert!(steps.iter().any(|step| step.contains("package(enshwick)")));
    assert!(!steps.iter().any(|step| step.contains("--test main")));
    assert!(!steps.iter().any(|step| step.contains("crates/enshwick/")));
    assert_eq!(windows_tests(&scope, &packages), Vec::<&str>::new());
}

#[test]
fn a_facade_change_with_many_dependents_leaves_trybuild_and_the_stable_examples_to_ci() {
    let args = stable();
    let scope = |tested: &[&str]| Scope::Some {
        code: ["enshwick".to_owned()].into(),
        docs: BTreeSet::new(),
        tested: tested.iter().map(|name| (*name).to_owned()).collect(),
        built: BTreeSet::new(),
        wide: !tested.is_empty(),
    };
    let steps = |scope: &Scope| -> Vec<String> {
        host_steps(
            scope,
            (&[], &args),
            (true, true, true),
            Some("package(enshwick)"),
            &[],
            false,
            &Part::every(false),
        )
        .iter()
        .map(Step::line)
        .collect()
    };
    let has = |steps: &[String], with: &str| steps.iter().any(|step| step.contains(with));
    let stable_line = args.join(" ");
    let narrow = steps(&scope(&[]));
    assert!(has(&narrow, "--test main"), "{narrow:?}");
    assert!(has(&narrow, &format!("cargo test --doc {stable_line}")));
    assert!(has(&narrow, &format!("cargo doc --no-deps {stable_line}")));
    let wide = steps(&scope(&["enshwick-loader", "enshwick-luau"]));
    assert!(!has(&wide, "--test main"), "{wide:?}");
    assert!(!has(&wide, &format!("cargo test --doc {stable_line}")));
    assert!(has(&wide, &format!("cargo doc --no-deps {stable_line}")));
    assert!(has(
        &wide,
        &format!("cargo clippy --all-targets {stable_line}")
    ));
    assert!(has(&wide, "cargo nextest"));
    assert!(
        !wide.iter().any(|step| step == "cargo doc --no-deps"),
        "{wide:?}"
    );
    assert!(has(&wide, "cargo doc --no-deps -p enshwick"), "{wide:?}");
    assert!(has(&wide, "doc-links"), "{wide:?}");
    // Every package it reaches changed: no dependent is only tested, and it is wide all the same.
    let changed = steps(&Scope::Some {
        code: ["enshwick", "enshwick-abi", "enshwick-loader"]
            .map(str::to_owned)
            .into(),
        docs: BTreeSet::new(),
        tested: BTreeSet::new(),
        built: BTreeSet::new(),
        wide: true,
    });
    assert!(!has(&changed, "--test main"), "{changed:?}");
    assert!(!has(&changed, &format!("cargo test --doc {stable_line}")));
    assert!(has(&changed, "cargo test --doc"));
}

/// A package at `dir`, with Windows tests when it builds only for Windows.
fn package(name: &str, dir: &str, windows_only: bool) -> Package {
    Package {
        name: name.to_owned(),
        version: "0.0.0-dev".to_owned(),
        dir: dir.to_owned(),
        windows_only,
        windows_tests: windows_only,
        ..Package::default()
    }
}

#[test]
fn a_changed_package_with_windows_tests_runs_them() {
    let packages = [package("enshwick-loader", "crates/runtime/loader", true)];
    let scope = Scope::Some {
        code: ["enshwick-loader".to_owned()].into(),
        docs: BTreeSet::new(),
        tested: BTreeSet::new(),
        built: BTreeSet::new(),
        wide: false,
    };
    assert_eq!(windows_tests(&scope, &packages), ["enshwick-loader"]);
    assert_eq!(windows_tests(&Scope::All, &packages), ["enshwick-loader"]);
}

#[test]
fn the_test_filter_names_each_package() {
    let names = ["a".to_owned(), "b".to_owned()];
    assert_eq!(test_filter(names.iter()), "package(a) | package(b)");
    assert_eq!(test_filter([].iter()), "");
}

#[test]
fn a_package_has_windows_code_when_it_is_windows_only_or_a_cfg_in_its_source_names_the_target() {
    let root = workspace_root();
    assert!(features::windows_code(
        &root,
        &package("stub", "no/such/dir", true)
    ));
    assert!(features::windows_code(
        &root,
        &package("loader", "crates/runtime/loader", false)
    ));
    assert!(!features::windows_code(
        &root,
        &package("values", "crates/internal/values", false)
    ));
    for text in [
        "#[cfg(windows)]",
        "#[cfg(not(windows))]",
        "#[cfg(all(test, target_os = \"windows\"))]",
        "fn f() { if cfg!(windows) {} }",
        "#[cfg_attr(windows, expect(dead_code))]",
        "#[cfg(unix)] mod paths;",
        "#[cfg(not(target_os = \"linux\"))] fn f() {}",
        "#[cfg(target_env = \"msvc\")] fn f() {}",
        "fn f() { \"open",
    ] {
        assert!(features::gates_target(text), "{text}");
    }
    for text in [
        "use windows_sys::Win32;",
        "/// Runs on Windows only, under #[cfg(windows)].",
        "const GATE: &str = \"cfg(windows)\";",
        "#[cfg_attr(feature = \"a\", doc = \")\")] fn windows() {}",
        "#[cfg(feature = \"test\")] mod windows_paths;",
        "fn mycfg() {} fn f() { mycfg(windows) }",
        "#[cfg(test)]\n#[expect(dead_code)]\nmod tests { #[cfg(windows)] fn f() {} }",
        "#[cfg(test)] use std::os::windows as _; #[cfg(test)] fn f() { if cfg!(unix) {} }",
        "cfg",
        "",
    ] {
        assert!(!features::gates_target(text), "{text}");
    }
    assert!(features::gates_target(
        "#[cfg(test)] mod tests;\n#[cfg(windows)] mod paths;"
    ));
}

#[test]
fn a_target_cfg_in_a_tests_file_or_folder_gives_a_package_no_windows_code() {
    let root = crate::scratch::Scratch::new("windows-code").unwrap();
    let write = |path: &str, text: &str| {
        let path = root.join("p/src").join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    };
    write("lib.rs", "#[cfg(test)]\nmod tests;\nmod a;\n");
    write("tests.rs", "#[cfg(windows)] fn f() {}");
    write("a.rs", "pub fn a() {}");
    write("a/tests/files.rs", "#[cfg(unix)] fn f() {}");
    let package = package("p", "p", false);
    assert!(!features::windows_code(&root, &package));
    write("a.rs", "#[cfg(unix)] pub fn a() {}");
    assert!(features::windows_code(&root, &package));
}

#[test]
fn a_tool_step_runs_its_program_and_an_xtask_step_this_binary() {
    assert_eq!(Step::tool("typos", &[]).program.as_deref(), Some("typos"));
    let exe = std::env::current_exe().unwrap().display().to_string();
    let step = Step::xtask(&["doc-links"]);
    assert_eq!(step.program.as_deref(), Some(exe.as_str()));
    assert_eq!(step.args, ["doc-links"]);
}

#[test]
fn host_args_leave_out_windows_only_crates_only_off_windows() {
    let packages = [
        package("api", "api", false),
        package("loader", "loader", true),
    ];
    assert_eq!(
        host_args(&packages, true),
        ["--workspace", "--features", TEST_FEATURE]
    );
    assert_eq!(
        host_args(&packages, false),
        [
            "--workspace",
            "--features",
            TEST_FEATURE,
            "--exclude",
            "loader"
        ]
    );
}

#[test]
fn every_host_build_lints_the_loader_s_benchmark_hooks_and_benchmarks() {
    assert!(TEST_FEATURE.ends_with(",enshwick-loader/test"));
}

#[test]
fn a_change_to_the_api_code_builds_its_stable_rustdoc() {
    let args = stable();
    assert!(runs_stable_rustdoc(
        &stable_api_steps(true, true, &args),
        &args
    ));
}

#[test]
fn a_change_to_the_book_alone_builds_the_stable_rustdoc() {
    let args = stable();
    assert!(runs_stable_rustdoc(
        &stable_api_steps(false, true, &args),
        &args
    ));
}

#[test]
fn a_change_to_code_builds_the_workspace_rustdoc_outside_the_full_suite() {
    assert!(workspace_rustdoc(true, false, false));
    assert!(workspace_rustdoc(false, false, true));
    assert!(!workspace_rustdoc(false, false, false));
}

#[test]
fn a_wide_change_leaves_the_workspace_rustdoc_to_the_full_suite() {
    assert!(!workspace_rustdoc(true, true, false));
    assert!(workspace_rustdoc(true, true, true));
}

#[test]
fn a_wide_change_documents_the_packages_it_changed_that_the_host_builds() {
    let scope = Scope::Some {
        code: ["enshwick-abi", "enshwick-loader-stub"]
            .map(str::to_owned)
            .into(),
        docs: BTreeSet::new(),
        tested: ["enshwick-ui".to_owned()].into(),
        built: BTreeSet::new(),
        wide: true,
    };
    let workspace = ["--workspace", "--exclude", "enshwick-loader-stub"].map(str::to_owned);
    let lines: Vec<String> = changed_rustdoc(&scope, &workspace)
        .iter()
        .map(Step::line)
        .collect();
    assert_eq!(lines, ["cargo doc --no-deps -p enshwick-abi"]);
    let stub_only = Scope::Some {
        code: ["enshwick-loader-stub".to_owned()].into(),
        docs: BTreeSet::new(),
        tested: BTreeSet::new(),
        built: BTreeSet::new(),
        wide: true,
    };
    assert!(changed_rustdoc(&stub_only, &workspace).is_empty());
}

#[test]
fn a_wide_change_to_the_facade_documents_it_with_the_features_of_a_workspace_build() {
    let scope = Scope::Some {
        code: [FACADE, "enshwick-base", "enshwick-luau"]
            .map(str::to_owned)
            .into(),
        docs: BTreeSet::new(),
        tested: BTreeSet::new(),
        built: BTreeSet::new(),
        wide: true,
    };
    let lines: Vec<String> = changed_rustdoc(&scope, &["--workspace".to_owned()])
        .iter()
        .map(Step::line)
        .collect();
    assert_eq!(
        lines[0],
        format!(
            "cargo doc --no-deps -p {FACADE} -p enshwick-base -p enshwick-luau --features {FACADE_FEATURES}"
        )
    );
    assert!(TEST_FEATURE.starts_with(FACADE_FEATURES));
    assert_eq!(lines[1], doc_links_step().line());
}

#[test]
fn a_wide_change_with_the_facade_left_out_of_the_build_checks_no_links_of_unwritten_pages() {
    let scope = Scope::Some {
        code: [FACADE, "enshwick-base"].map(str::to_owned).into(),
        docs: BTreeSet::new(),
        tested: BTreeSet::new(),
        built: BTreeSet::new(),
        wide: true,
    };
    let workspace = ["--workspace", "--exclude", FACADE].map(str::to_owned);
    let lines: Vec<String> = changed_rustdoc(&scope, &workspace)
        .iter()
        .map(Step::line)
        .collect();
    assert_eq!(lines, ["cargo doc --no-deps -p enshwick-base"]);
}

#[test]
fn the_public_paths_list_is_written_again_only_when_its_writer_or_the_list_changes() {
    let scope = |code: &[&str]| Scope::Some {
        code: code.iter().map(|name| (*name).to_owned()).collect(),
        docs: BTreeSet::new(),
        tested: BTreeSet::new(),
        built: BTreeSet::new(),
        wide: false,
    };
    let list = [public_paths::FILE.to_owned()];
    let other = ["README.md".to_owned()];
    assert!(lists_public_paths(&scope(&[FACADE]), Some(&other)));
    assert!(lists_public_paths(
        &scope(&[env!("CARGO_PKG_NAME")]),
        Some(&other)
    ));
    assert!(lists_public_paths(&scope(&[]), Some(&list)));
    let chapter = [public_paths::chapter::FILE.to_owned()];
    assert!(!lists_public_paths(&scope(&[]), Some(&chapter)));
    assert!(!lists_public_paths(
        &scope(&["enshwick-loader"]),
        Some(&other)
    ));
    assert!(!lists_public_paths(&scope(&[]), None));
}

#[test]
fn the_stable_doctests_skip_unstable_modules_under_either_path_separator() {
    let steps = stable_api_steps(false, true, &stable());
    let skipped = |name: &str| {
        steps.iter().any(|step| {
            step.args
                .windows(2)
                .any(|pair| pair[0] == "--skip" && name.contains(pair[1].as_str()))
        })
    };
    for file in [
        "core/src/script/mod.rs",
        "core/src/rates.rs",
        "core/src/item_data.rs",
        "base/src/script.rs",
        "values/src/script/objects.rs",
    ] {
        let name = format!("crates/internal/{file} - item (line 1)");
        assert!(skipped(&name), "{name}");
        assert!(skipped(&name.replace('/', "\\")), "{name}");
    }
    assert!(skipped(
        "crates/internal/core/src/lib.rs - MergedExamples (line 12)"
    ));
    assert!(!skipped("crates\\enshwick\\src\\lib.rs - item (line 1)"));
}

mod docs;
mod feature_lints;
mod targets;
