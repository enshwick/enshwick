use std::collections::BTreeSet;

use clap::ValueEnum as _;

use super::*;

#[test]
fn the_docs_trybuild_and_semver_parts_parse_and_run_when_no_part_is_named() {
    assert_eq!(Part::from_str("docs", false), Ok(Part::Docs));
    assert_eq!(
        Part::from_str("windows-tests", false),
        Ok(Part::WindowsTests)
    );
    assert_eq!(Part::from_str("trybuild", false), Ok(Part::Trybuild));
    assert_eq!(Part::from_str("semver", false), Ok(Part::Semver));
    for part in [Part::Docs, Part::Trybuild, Part::Semver] {
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
        built: BTreeSet::new(),
    };
    let filter = code
        .iter()
        .map(|name| format!("package({name})"))
        .collect::<Vec<_>>()
        .join(" | ");
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
fn the_host_and_docs_parts_split_the_steps_a_check_of_every_part_runs_in_order() {
    let host = planned(&[Part::Host, Part::Trybuild]);
    let docs = planned(&[Part::Docs]);
    let starts = |steps: &[String], with: &str| steps.iter().any(|s| s.starts_with(with));
    assert!(starts(&host, "cargo nextest") && starts(&host, "cargo deny"));
    assert!(!starts(&host, "cargo test --doc") && !starts(&host, "cargo doc"));
    assert!(starts(&docs, "cargo test --doc") && starts(&docs, "cargo doc"));
    assert!(!starts(&docs, "cargo nextest") && !starts(&docs, "cargo fmt"));
    let every = planned(&Part::every(false));
    // A step that runs twice, such as the link check after each docs
    // build, takes its next place in the full run.
    let order = |steps: &[String]| -> Vec<usize> {
        let mut taken: Vec<usize> = Vec::new();
        for step in steps {
            let at = (0..every.len())
                .find(|at| every[*at] == *step && !taken.contains(at))
                .unwrap();
            taken.push(at);
        }
        taken
    };
    assert_eq!(every.len(), host.len() + docs.len());
    assert!(order(&host).is_sorted() && order(&docs).is_sorted());
}

#[test]
fn a_change_to_the_api_docs_alone_plans_the_stable_steps_in_the_docs_part() {
    let docs_of = |name: &str| Scope::Some {
        code: BTreeSet::new(),
        docs: [name.to_owned()].into(),
        built: BTreeSet::new(),
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
            &[Part::Docs],
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
        built: BTreeSet::new(),
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
        built: ["enshwick-core".to_owned()].into(),
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
    ] {
        assert!(named(name), "{name}");
    }
    for name in ["enshwick-values", "enshwick-abi", "enshwick-luau", "xtask"] {
        assert!(!named(name), "{name}");
    }
    assert!(stable.ends_with(&["--features".to_owned(), "enshwick/test".to_owned()]));
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
    let package = |name: &str, windows_only| Package {
        name: name.to_owned(),
        version: "0.0.0-dev".to_owned(),
        dir: name.to_owned(),
        deps: Vec::new(),
        lib: None,
        windows_only,
        windows_tests: windows_only,
        unstable: false,
    };
    let packages = [package("api", false), package("loader", true)];
    let base = ["--workspace", "--features", TEST_FEATURE];
    assert_eq!(host_args(&packages, true), base);
    assert_eq!(
        host_args(&packages, false),
        [&base[..], &["--exclude", "loader"]].concat()
    );
}

#[test]
fn a_change_to_the_api_code_builds_its_stable_rustdoc() {
    let args = stable();
    assert!(runs_stable_rustdoc(&stable_api_steps(true, &args), &args));
}

#[test]
fn a_change_to_the_book_alone_builds_the_stable_rustdoc() {
    let args = stable();
    assert!(runs_stable_rustdoc(&stable_api_steps(false, &args), &args));
}

#[test]
fn a_change_to_code_builds_the_workspace_rustdoc_outside_the_full_suite() {
    assert!(workspace_rustdoc(true, false));
    assert!(workspace_rustdoc(false, true));
    assert!(!workspace_rustdoc(false, false));
}

#[test]
fn the_stable_doctests_skip_unstable_modules_under_either_path_separator() {
    let steps = stable_api_steps(false, &stable());
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
        "base/src/services/calls.rs",
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
