//! `cargo xtask check`: the compiling checks a change needs before it
//! merges; the git hooks run only the checks that compile nothing.
//!
//! Nothing runs when no package changed since the branch left `origin/main`.
//! Otherwise the tree's own rules run ([`crate::check`]), then clippy and
//! rustdoc over the workspace, where unchanged packages are already fresh,
//! and nextest and the doctests run for the changed packages and every
//! package that depends on them, after clippy of each checked package with
//! none and each one of its features ([`features`], a part of its own in CI); a local check leaves the widest part of
//! that to CI ([`affected::Lane`]) and runs the tests a wide change only
//! reaches one process per binary ([`in_process`]). Off Windows the workspace is also
//! linted for Windows through `cargo xwin`, beside the host checks, and
//! then each checked package with Windows code with none and each one of
//! its features alone (a part of its own in CI).
//! `--windows` also runs the Windows tests of the crates marked
//! `windows-tests` or `windows-only`, under Wine on an amd64 Linux host.
//! `--all` runs the full suite, the Windows tests included where they can
//! run, but no fuzzing. `--part fuzz` runs the fuzz targets the change
//! reaches for a short while, or with `--all` every target
//! ([`super::fuzz`]). `--part trybuild` runs the trybuild test only when
//! the API crate's tests are in scope, and `--part semver` runs
//! cargo-semver-checks for the packages `--semver` names.

use std::io::Write as _;
use std::path::Path;
use std::process::Command;
use std::thread;

use crate::affected::{self, Lane, Package, Scope, WIDE_PACKAGES};
use crate::commands::{fuzz, header, public_paths, wine};
use crate::{FACADE, packages, say, workspace_root};
use part::{
    Part, due_parts, facade_code_reached, stable_api_due, stable_api_out_of_scope,
    stable_examples_left_to_ci, trybuild_due, windows_features_due,
};

pub(crate) use step::Step;

pub(crate) const WINDOWS_TARGET: &str = "x86_64-pc-windows-msvc";
macro_rules! facade_features {
    () => {
        "enshwick/test,enshwick/unstable,enshwick/typed"
    };
}
/// Turned on by name: most of the API crate's test targets need its fake
/// loader, and without the feature cargo skips them without a word; the
/// API's unstable calls, its generated typed API, the compat task and the
/// loader's benchmark hooks with their benchmarks are linted and tested with
/// the rest.
const TEST_FEATURE: &str = concat!(facade_features!(), ",xtask/compat,enshwick-loader/test");
/// The API crate's part of [`TEST_FEATURE`], which a build that selects the
/// facade without `xtask` takes so its features unify as in a workspace build.
const FACADE_FEATURES: &str = facade_features!();

/// The nextest filter of the trybuild test, which builds the API crate again
/// under `target/tests/trybuild`: only [`Part::Trybuild`] runs it.
const TRYBUILD: &str = "binary_id(enshwick::main) & test(/^(config_files|internals)::/)";

/// The API crate's features the trybuild test builds with: trybuild builds
/// its cases with the features of the test, and none of them needs the
/// generated typed API, the costliest part of the crate to build.
const TRYBUILD_FEATURES: &str = "test,unstable";

/// The command line `check` takes.
#[derive(Debug, clap::Args)]
pub(crate) struct Args {
    /// Runs the full suite: every package, `cargo fmt` and `cargo deny`, and
    /// the Windows tests; fuzzes only with `--part fuzz`.
    #[arg(long)]
    all: bool,
    /// Also runs the Windows tests of the changed packages.
    #[arg(long)]
    windows: bool,
    /// The git ref whose merge base the change is measured from.
    #[arg(long, default_value = "origin/main")]
    base: String,
    /// Runs only these parts, so CI can run them as parallel jobs.
    #[arg(long, value_enum)]
    part: Vec<Part>,
    /// A package the semver part checks against its last release on
    /// crates.io; repeatable, and the part checks none without one.
    #[arg(long, value_name = "PACKAGE")]
    semver: Vec<String>,
    /// Lists what the change needs, one a line, and checks nothing.
    #[arg(long, value_enum)]
    list: Option<Listing>,
}

/// What `--list` names, for CI to start a job only where one has work.
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
enum Listing {
    /// The packages whose Windows tests the change reaches, which CI tests
    /// on a Windows runner.
    WindowsTests,
    /// The parts named with `--part` that may have a step for the change:
    /// the trybuild test and the stable surface only where the change
    /// reaches them, the Windows feature lints only where it checks a package
    /// with features and Windows code; every other part is listed, since it
    /// finds its own work only when it runs.
    Parts,
}

/// The crates of the API ([`crate::check::crate_rules::is_api_crate`]) with
/// a stable surface of their own ([`Package::stable_build`]), each a
/// `--package` argument, with the API crate's `test` feature and without
/// `unstable`: the workspace turns the feature on for the runtimes, so only
/// a build of their own shows that the stable surface, its docs and its
/// examples stand without the unstable one.
pub(crate) fn stable_api(packages: &[Package]) -> Vec<String> {
    let mut args = Vec::new();
    for package in packages.iter().filter(|p| {
        p.stable_build && crate::check::crate_rules::is_api_crate(&p.name, Path::new(&p.dir))
    }) {
        args.extend(["--package".to_owned(), package.name.clone()]);
    }
    args.extend(["--features".to_owned(), format!("{FACADE}/test")]);
    args
}

/// Runs the check `args` asks for; whether it passed.
///
/// # Errors
///
/// When `cargo metadata` or a rule's input does not read.
pub(crate) fn run(args: &Args) -> Result<bool, String> {
    let parts = if args.part.is_empty() {
        Part::every(args.all || args.windows)
    } else {
        args.part.clone()
    };
    let merge_base = merge_base(&args.base);
    let changed = merge_base.as_deref().and_then(changed_files);
    let listing = args.list.is_some();
    if parts.contains(&Part::Fuzz) && !listing {
        let fuzzed = fuzz_part(changed.as_deref(), args.all);
        if !fuzzed || parts.len() == 1 {
            return Ok(fuzzed);
        }
    }
    let packages = packages()?;
    let root = workspace_root();
    if parts.contains(&Part::Host) && !listing {
        let mut problems = crate::check::problems(&root, &packages)?;
        problems.extend(
            merge_base
                .as_deref()
                .and_then(name_status)
                .iter()
                .flat_map(|status| crate::check::changelog::edited(status))
                .map(|problem| format!("changelog: {problem}")),
        );
        for problem in &problems {
            say(problem);
        }
        if !problems.is_empty() {
            return Ok(false);
        }
    }
    let scope = if args.all {
        Scope::All
    } else if let (Some(files), Some(merge_base)) = (&changed, merge_base) {
        affected::scope(
            files,
            &packages,
            &Tree {
                merge_base,
                root: &root,
            },
            Lane::current(),
        )
    } else {
        say(&format!(
            "no merge base with {}; checking everything",
            args.base
        ));
        Scope::All
    };
    let windows_tests = windows_tests(&scope, &packages);
    if let Some(listing) = args.list {
        let names = match listing {
            Listing::WindowsTests => windows_tests.iter().map(|&name| name.to_owned()).collect(),
            Listing::Parts => listed_parts(&parts, &scope, &packages, &root),
        };
        for name in names {
            let _shown = writeln!(std::io::stdout(), "{name}");
        }
        return Ok(true);
    }
    // It compiles nothing, so it runs wherever the change reaches the facade.
    if parts.contains(&Part::Host) && reaches(&scope, header::PACKAGE) {
        let problems = header::check(&root);
        for problem in &problems {
            say(&format!("C header: {problem}"));
        }
        if !problems.is_empty() {
            return Ok(false);
        }
    }
    if parts.contains(&Part::Host)
        && !public_paths_hold(&scope, changed.as_deref(), &root, &packages)
    {
        return Ok(false);
    }
    if parts.contains(&Part::Trybuild) && !trybuild_due(&scope) {
        say(&if touches(&scope, FACADE) {
            format!(
                "the trybuild test is left to CI: the change reaches {WIDE_PACKAGES} or more packages"
            )
        } else if matches!(&scope, Scope::Some { tested, .. } if tested.contains(FACADE)) {
            format!("the trybuild test is left to CI: only {FACADE}'s tests run here")
        } else if reaches(&scope, FACADE) {
            format!("the trybuild test is left to CI: {FACADE} is only built here")
        } else {
            format!("the trybuild test is out of scope: the change does not reach {FACADE}")
        });
    }
    let checked = check(&scope, &packages, args.all, &parts)
        && (!parts.contains(&Part::Semver) || semver(&args.semver));
    if !(checked && parts.contains(&Part::WindowsTests)) {
        return Ok(checked);
    }
    if windows_tests.is_empty() {
        say("no package with Windows tests changed; no tests under Wine");
        return Ok(true);
    }
    Ok(wine::run(&windows_tests, args.windows))
}

/// The names of the `parts` with a step for the change `scope` to the
/// `packages` under `root` ([`due_parts`]).
fn listed_parts(parts: &[Part], scope: &Scope, packages: &[Package], root: &Path) -> Vec<String> {
    let windows_features = windows_features_due(scope, packages, windows_code(root));
    due_parts(parts, scope, windows_features)
}

/// Whether a package under `root` has Windows code ([`features::windows_code`]),
/// the one rule the Windows feature lints and their listing share.
fn windows_code(root: &Path) -> impl Fn(&Package) -> bool + '_ {
    move |package| features::windows_code(root, package)
}

/// Whether the committed public paths list is current and the book's
/// stability chapter agrees with it. A wide change checks it too: it
/// documents the facade in a target of its own, 79 s from clean and 42 s
/// after an ABI change (2026-11-18), and a stale list was the most common
/// red CI run of wide changes.
fn public_paths_hold(
    scope: &Scope,
    changed: Option<&[String]>,
    root: &Path,
    packages: &[Package],
) -> bool {
    if !lists_public_paths(scope, changed) {
        let problems = if names_file(changed, public_paths::chapter::FILE) {
            public_paths::check_chapter(root)
        } else {
            Vec::new()
        };
        for problem in &problems {
            say(&format!("public paths: {problem}"));
        }
        return problems.is_empty();
    }
    let problems = public_paths::check(root, packages);
    for problem in &problems {
        say(&format!("public paths: {problem}"));
    }
    problems.is_empty()
}

/// Whether a change can move [`public_paths::FILE`], so the facade is
/// documented again to check it: it reaches the facade, changes this task,
/// which writes the list, or edits the list, which no package owns. An edit
/// of the book's stability chapter alone is checked against the committed
/// list.
fn lists_public_paths(scope: &Scope, changed: Option<&[String]>) -> bool {
    reaches(scope, FACADE)
        || touches(scope, env!("CARGO_PKG_NAME"))
        || names_file(changed, public_paths::FILE)
}

/// Whether the changed files `changed` hold `file`.
fn names_file(changed: Option<&[String]>, file: &str) -> bool {
    changed.is_some_and(|files| files.iter().any(|changed| changed == file))
}

/// Whether `scope` checks the code of `package`.
fn touches(scope: &Scope, package: &str) -> bool {
    match scope {
        Scope::All => true,
        Scope::Some { code, .. } => code.iter().any(|name| name == package),
    }
}

/// Whether a local check of `scope` reaches [`WIDE_PACKAGES`] or more
/// packages: it leaves the trybuild test and the stable surface's examples
/// to CI, which alone kept a check of a facade change over ten minutes
/// (669 s, 2026-11-14), but lints and documents that surface; it leaves the
/// dependents' rustdoc
/// ([`changed_rustdoc`]), and runs the dependents' tests one process per
/// binary ([`in_process`]).
fn wide(scope: &Scope) -> bool {
    matches!(scope, Scope::Some { wide: true, .. })
}

/// Whether `scope` checks, tests or only builds the code of `package`.
fn reaches(scope: &Scope, package: &str) -> bool {
    match scope {
        Scope::All => true,
        Scope::Some {
            code,
            tested,
            built,
            ..
        } => code.contains(package) || tested.contains(package) || built.contains(package),
    }
}

fn semver(packages: &[String]) -> bool {
    if let Some(step) = semver_step(packages) {
        step.run(false)
    } else {
        say("no package for cargo-semver-checks; --semver names them");
        true
    }
}

/// The cargo-semver-checks run of `packages`; none without one.
fn semver_step(packages: &[String]) -> Option<Step> {
    (!packages.is_empty()).then(|| {
        packages
            .iter()
            .fold(Step::new(&["semver-checks"]), |step, package| {
                step.then(&["--package", package])
            })
    })
}

/// The fuzz part: the targets the `changed` files reach, or every target in
/// the full suite or without a merge base; whether none crashed.
fn fuzz_part(changed: Option<&[String]>, all: bool) -> bool {
    let targets = match changed {
        Some(files) if !all => fuzz::reached(files),
        _ => fuzz::all(),
    };
    fuzz::run(&targets)
}

/// Files changed between `merge_base` and the working tree, untracked ones
/// included; `None` when git fails.
fn changed_files(merge_base: &str) -> Option<Vec<String>> {
    let mut files: Vec<String> = git(&["diff", "--name-only", merge_base])?
        .lines()
        // `:/` and --full-name: every untracked file, named from the root
        // as diff names it, wherever the check runs from.
        .chain(git(&["ls-files", "--others", "--exclude-standard", "--full-name", ":/"])?.lines())
        .map(str::to_owned)
        .collect();
    files.sort();
    files.dedup();
    Some(files)
}

/// What changed between `merge_base` and the working tree, as
/// `git diff --name-status` lists it; `None` when git fails.
fn name_status(merge_base: &str) -> Option<String> {
    git(&["diff", "--name-status", merge_base])
}

/// The merge base of `HEAD` with `base`.
fn merge_base(base: &str) -> Option<String> {
    Some(git(&["merge-base", "HEAD", base])?.trim().to_owned())
}

/// The repository between its merge base and the working tree.
struct Tree<'a> {
    merge_base: String,
    root: &'a Path,
}

impl affected::Texts for Tree<'_> {
    fn before(&self, path: &str) -> Option<String> {
        git(&["show", &format!("{}:{path}", self.merge_base)])
    }

    fn after(&self, path: &str) -> Option<String> {
        std::fs::read_to_string(self.root.join(path)).ok()
    }
}

pub(crate) fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

/// The libtest filters that pick the examples of `names` out of a workspace
/// doctest run: rustdoc names each example after its file, so a package's
/// directory with a trailing separator, in either form, matches its own
/// examples and the book chapters it includes, and no other package's.
fn doctest_filters<'a>(
    names: impl Iterator<Item = &'a String>,
    packages: &[Package],
) -> Vec<String> {
    let mut filters = Vec::new();
    for package in names.filter_map(|name| packages.iter().find(|p| &p.name == name)) {
        let dir = format!("{}/", package.dir);
        filters.push(dir.replace('/', "\\"));
        filters.push(dir);
    }
    filters.sort();
    filters.dedup();
    filters
}

/// The nextest filter that runs the tests of `names`; empty, which runs
/// none, without one.
fn test_filter<'a>(names: impl Iterator<Item = &'a String>) -> String {
    names
        .map(|name| format!("package({name})"))
        .collect::<Vec<_>>()
        .join(" | ")
}

/// The flags, nextest filter and doctest filters [`host_steps`] takes for
/// `scope`: the examples of the packages it only tests stay out.
/// `host_ok` says whether a package's tests run on this host.
fn plan(
    scope: &Scope,
    packages: &[Package],
    host_ok: impl Fn(&String) -> bool,
) -> ((bool, bool, bool), Option<String>, Vec<String>) {
    let Scope::Some {
        code,
        docs,
        tested,
        built,
        ..
    } = scope
    else {
        return ((true, true, true), None, Vec::new());
    };
    let host: Vec<&String> = code.iter().filter(|name| host_ok(name)).collect();
    let filter = test_filter(
        host.iter()
            .copied()
            .chain(tested.iter().filter(|name| host_ok(name))),
    );
    let doctests = doctest_filters(host.into_iter().chain(docs), packages);
    (
        (
            !code.is_empty() || !tested.is_empty() || !built.is_empty(),
            !code.is_empty(),
            !doctests.is_empty(),
        ),
        Some(filter),
        doctests,
    )
}

/// The packages of `scope` whose Windows tests run: those it checks in
/// full. Under Wine the tests run one at a time with their examples, so a
/// package only tested leaves them to CI.
fn windows_tests<'a>(scope: &Scope, packages: &'a [Package]) -> Vec<&'a str> {
    packages
        .iter()
        .filter(|p| p.windows_tests)
        .filter(|p| match scope {
            Scope::All => true,
            Scope::Some { code, docs, .. } => code.contains(&p.name) || docs.contains(&p.name),
        })
        .map(|p| p.name.as_str())
        .collect()
}

/// The checks of the API crate's stable surface, built without `unstable`:
/// its lints when `lint`, its examples when `examples`, and its rustdoc
/// always. `stable` names the crates and features ([`stable_api`]).
fn stable_api_steps(lint: bool, examples: bool, stable: &[String]) -> Vec<Step> {
    let mut steps = Vec::new();
    if lint {
        steps.push(
            Step::new(&["clippy", "--all-targets"])
                .then_owned(stable)
                .then(&["--", "-D", "warnings"]),
        );
    }
    // The script binding, the rates and item data are public only with
    // `unstable`, so their examples fail to build
    // without it. They are marked `standalone_crate` and skipped here: one
    // example that fails in the merged build makes rustdoc build the rest
    // one by one, which takes minutes; merged they share one binary and take
    // seconds. The test names carry the source path with the host's
    // separator.
    let mut doctests = Step::new(&["test", "--doc"])
        .then_owned(stable)
        .then(&["--"]);
    for module in ["src/script", "src/rates.rs", "src/item_data.rs"] {
        let windows = module.replace('/', "\\");
        doctests = doctests.then(&["--skip", module, "--skip", &windows]);
    }
    // Without `unstable` the examples of the unstable surface skipped above
    // still keep content, core and inventory from merging; the sentinels hold
    // the workspace's run, where every crate merges.
    doctests = doctests.then(&["--skip", crate::check::SENTINEL_NAME]);
    if examples {
        steps.push(doctests);
    }
    // A link into an unstable module breaks only this build: 13 s from
    // clean, 14 s after an ABI change (2026-11-18).
    steps.push(
        Step::new(&["doc", "--no-deps"])
            .then_owned(stable)
            .env("RUSTDOCFLAGS", "-D warnings"),
    );
    steps.push(doc_links_step());
    steps
}

/// The check of the links rustdoc does not check in the facade's pages it
/// just wrote ([`super::doc_links`]).
fn doc_links_step() -> Step {
    Step::xtask(&["doc-links"])
}

/// Whether the workspace's rustdoc runs: whenever code changed, since a
/// change to one crate breaks links in the docs of the crates that use it,
/// and in the full suite. A [`wide`] change leaves the dependents' rustdoc
/// to CI with their examples ([`changed_rustdoc`]): the workspace's took
/// 78 s of a check that took 11 min 45 s after an ABI change while four
/// batches built (2026-11-16).
const fn workspace_rustdoc(code: bool, wide: bool, full: bool) -> bool {
    (code && !wide) || full
}

/// The rustdoc of the packages a [`wide`] `scope` changed, those the host
/// build `workspace` leaves out skipped, with the link check of the
/// facade's pages when it is one: 11 s the first time, then under a second
/// (2026-11-17). It selects them alone; with the facade among them it turns
/// on the facade's features as a workspace build does, since a selected
/// runtime such as `enshwick-luau` would otherwise turn on an internal
/// crate's `unstable` without the facade's and break its links.
fn changed_rustdoc(scope: &Scope, workspace: &[String]) -> Vec<Step> {
    let Scope::Some { code, .. } = scope else {
        return Vec::new();
    };
    let excluded = |name: &String| {
        workspace
            .windows(2)
            .any(|pair| pair[0] == EXCLUDE && &pair[1] == name)
    };
    let mut step = Step::new(&["doc", "--no-deps"]).env("RUSTDOCFLAGS", "-D warnings");
    let mut any = false;
    for name in code.iter().filter(|name| !excluded(name)) {
        step = step.then(&["-p", name.as_str()]);
        any = true;
    }
    let facade = touches(scope, FACADE);
    if facade {
        step = step.then(&["--features", FACADE_FEATURES]);
    }
    let mut steps = Vec::new();
    if any {
        steps.push(step);
    }
    if facade {
        steps.push(doc_links_step());
    }
    steps
}

/// The target CI's host checks build for.
const LINUX_TARGET: &str = "x86_64-unknown-linux-gnu";

/// The clippy of the workspace's libraries and binaries for Linux, which a
/// local check on Windows runs when `scope` reaches a package it lints: CI
/// lints on Linux, where target gates (in the package, its manifest or a
/// dependency) leave imports and items unused or unresolved that the
/// Windows build uses (four red runs, 2026-11-16 to 17); 11 s from clean,
/// 10 s after an ABI change. The other targets
/// stay out, since their dev-dependencies (criterion) compile C, and so do
/// the packages that compile C ([`Package::compiles_c`]): both need a Linux
/// cross compiler, so CI lints them.
fn linux_lint(scope: &Scope, packages: &[Package]) -> Option<Step> {
    packages
        .iter()
        .filter(|package| !package.windows_only && !package.compiles_c)
        .any(|package| reaches(scope, &package.name))
        .then(|| {
            let mut step = Step::new(&["clippy", "--target", LINUX_TARGET])
                .then_owned(&host_args(packages, false));
            for package in packages.iter().filter(|package| package.compiles_c) {
                step = step.then(&[EXCLUDE, &package.name]);
            }
            step.then(&["--", "-D", "warnings"])
        })
}

/// The [`linux_lint`] of a local check on Windows that `lints`, or none; an
/// error naming the command that installs the Linux standard library when it
/// is due without one.
fn local_linux_lint(
    scope: &Scope,
    packages: &[Package],
    lints: bool,
) -> Result<Option<Step>, String> {
    if !(cfg!(windows) && lints && Lane::current() == Lane::Local) {
        return Ok(None);
    }
    match linux_lint(scope, packages) {
        Some(_) if !linux_std_installed() => Err(format!(
            "the Linux lint needs the Linux standard library: run `rustup target add {LINUX_TARGET}` once"
        )),
        step => Ok(step),
    }
}

/// Whether the pinned toolchain has the standard library for
/// [`LINUX_TARGET`]: it stays out of `rust-toolchain.toml`, which every new
/// mod copies, since only a Windows host's check needs it.
fn linux_std_installed() -> bool {
    Command::new("rustc")
        .args(["--print", "target-libdir", "--target", LINUX_TARGET])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .is_some_and(|output| Path::new(String::from_utf8_lossy(&output.stdout).trim()).is_dir())
}

/// The ast-grep rules' own tests, the rules over the repository
/// ([`crate::check::rules`]), then the dependencies no crate uses.
fn source_checks() -> bool {
    Step::tool("ast-grep", &["test", "--skip-snapshot-tests"]).run(false)
        && crate::check::rules::scan(&workspace_root())
        && Step::tool("cargo-machete", &[]).run(false)
}

/// Cargo's flag that leaves a package out of a `--workspace` build.
const EXCLUDE: &str = "--exclude";

/// The cargo arguments every host build and test of the workspace takes.
pub(crate) fn host_args(packages: &[Package], on_windows: bool) -> Vec<String> {
    let mut args = vec![
        "--workspace".to_owned(),
        "--features".to_owned(),
        TEST_FEATURE.to_owned(),
    ];
    // Off Windows the crates that build only for Windows stay out of host builds.
    if !on_windows {
        for package in packages.iter().filter(|p| p.windows_only) {
            args.extend([EXCLUDE.to_owned(), package.name.clone()]);
        }
    }
    args
}

/// The host steps of `parts`, one after another, in the order a check of
/// every part runs them: `lint`, `code` and `docs` say what to build and
/// lint, what changed and whose examples changed; `tests` is the nextest
/// filter of the packages to test (`None`: every test) and `doctests` the
/// test name filters of the packages whose examples run (none: every
/// example); `workspace` and `stable` are the arguments of a workspace
/// build and of the stable API's ([`host_args`], [`stable_api`]).
fn host_steps(
    scope: &Scope,
    (workspace, stable): (&[String], &[String]),
    (lint, code, docs): (bool, bool, bool),
    tests: Option<&str>,
    doctests: &[String],
    full: bool,
    parts: &[Part],
) -> Vec<Step> {
    let host = |words: &[&str]| Step::new(words).then_owned(workspace);
    let mut steps = Vec::new();
    let run_host = parts.contains(&Part::Host);
    let run_docs = parts.contains(&Part::Docs);
    if full && run_host {
        steps.push(Step::new(&["fmt", "--all", "--check"]));
    }
    if lint && run_host {
        steps.push(host(&["clippy", "--all-targets"]).then(&["--", "-D", "warnings"]));
        // CI runs each part in a call of its own, so the host part never
        // runs the trybuild test, whatever other parts this command runs.
        let filter = match tests {
            Some("") => None,
            Some(filter) => Some(format!("({filter}) & not ({TRYBUILD})")),
            None => Some(format!("not ({TRYBUILD})")),
        };
        if let Some(filter) = filter {
            steps.push(host(&["nextest", "run", "--no-tests=pass"]).then(&["-E", &filter]));
        }
    }
    if parts.contains(&Part::Trybuild) && trybuild_due(scope) {
        steps.push(Step::new(&[
            "nextest",
            "run",
            "--package",
            FACADE,
            "--test",
            "main",
            "--features",
            TRYBUILD_FEATURES,
            "--no-tests=fail",
            "-E",
            TRYBUILD,
        ]));
    }
    if docs && run_docs {
        // A name filter, not `--package`: the build keeps the workspace's
        // features, so nothing builds twice.
        let mut step = host(&["test", "--doc"]);
        if !doctests.is_empty() {
            step = step.then(&["--"]).then_owned(doctests);
        }
        steps.push(step);
    }
    if parts.contains(&Part::StableApi) && stable_api_due(scope) {
        steps.extend(stable_api_steps(
            facade_code_reached(scope),
            !wide(scope),
            stable,
        ));
    }
    if run_docs && workspace_rustdoc(code, wide(scope), full) {
        // `code` alone: a package whose version alone moved needs no docs.
        steps.push(host(&["doc", "--no-deps"]).env("RUSTDOCFLAGS", "-D warnings"));
        steps.push(doc_links_step());
    } else if run_docs && code && wide(scope) {
        steps.extend(changed_rustdoc(scope, workspace));
    }
    if full && run_host {
        steps.push(Step::new(&["deny", "check"]));
    }
    steps
}

/// Runs the checks of `scope`. Every build selects the whole workspace, so
/// features unify the same way on every run and nothing builds twice; only
/// the tests and examples run are narrowed to the scope's packages. A
/// narrowed `cargo doc --package` would unify features differently and
/// rebuild their dependencies on each switch (an `xtask` alone 34 s, then the
/// workspace 64 s, 2026-11-14), while the workspace's rustdoc redoes only the
/// changed packages and their dependents. Host steps run one
/// after another (they share the build lock); the Windows lint runs beside
/// them.
fn check(scope: &Scope, packages: &[Package], full: bool, parts: &[Part]) -> bool {
    // The source checks read every file in about a second without compiling,
    // so they run whatever changed: a change to a rule alone touches no package.
    if parts.contains(&Part::Host) && !source_checks() {
        return false;
    }
    let on_windows = cfg!(windows);
    let host_ok =
        |name: &String| on_windows || packages.iter().any(|p| &p.name == name && !p.windows_only);
    if let Scope::Some {
        code,
        docs,
        tested,
        built,
        ..
    } = scope
    {
        if code.is_empty() && docs.is_empty() && tested.is_empty() && built.is_empty() {
            say("no package changed; nothing to check");
            return true;
        }
        let names: Vec<&str> = code.iter().chain(docs).map(String::as_str).collect();
        if !names.is_empty() {
            say(&format!("checking {}", names.join(", ")));
        }
        if !tested.is_empty() {
            let names: Vec<&str> = tested.iter().map(String::as_str).collect();
            say(&format!(
                "running only the tests of {}: CI and the merge queue run their examples",
                names.join(", "),
            ));
        }
        if wide(scope) && parts.contains(&Part::Docs) {
            say(&format!(
                "the dependents' rustdoc is left to CI: the change reaches {WIDE_PACKAGES} or more packages"
            ));
        }
        if stable_examples_left_to_ci(scope, parts) {
            say(&format!(
                "{FACADE}'s stable examples are left to CI: the change reaches {WIDE_PACKAGES} or more packages"
            ));
        } else if stable_api_out_of_scope(scope, parts) {
            say(&format!(
                "{FACADE}'s stable surface is out of scope: the change does not reach {FACADE}"
            ));
        }
        if !built.is_empty() {
            let built: Vec<&str> = built.iter().map(String::as_str).collect();
            say(&format!(
                "only building and linting {}: CI and the merge queue test them",
                built.join(", ")
            ));
        }
    } else {
        say("checking the whole workspace");
    }
    let separate = in_process::packages(scope, parts.contains(&Part::Host), packages, host_ok);
    let (flags, tests, doctests) = plan(scope, packages, |name| {
        host_ok(name) && !separate.contains(name)
    });
    let workspace = host_args(packages, on_windows);
    let stable = stable_api(packages);
    // The feature lints go first: they are cheap, and the first failing
    // step ends the host checks.
    let mut host_run = if parts.contains(&Part::Features) {
        features::steps(scope, packages, |p| host_ok(&p.name), &["clippy"])
    } else {
        Vec::new()
    };
    match local_linux_lint(scope, packages, flags.0 && parts.contains(&Part::Host)) {
        Ok(step) => host_run.extend(step),
        Err(problem) => {
            say(&problem);
            return false;
        }
    }
    host_run.extend(host_steps(
        scope,
        (&workspace, &stable),
        flags,
        tests.as_deref(),
        &doctests,
        full,
        parts,
    ));
    let windows = if !on_windows && flags.0 {
        let root = workspace_root();
        windows_lint::steps(scope, packages, parts, windows_code(&root))
    } else {
        Vec::new()
    };

    for step in &windows {
        say(&format!("> {} (beside the host checks)", step.line()));
    }
    thread::scope(|scope| {
        // The first failing step of either side ends that side's checks.
        let windows = (!windows.is_empty())
            .then(|| scope.spawn(|| windows.iter().all(|step| step.run(true))));
        let host_passed = host_run.iter().all(|step| step.run(false))
            && (separate.is_empty() || in_process::run(&separate, &workspace));
        let windows_passed = windows.is_none_or(|handle| handle.join().unwrap_or(false));
        host_passed && windows_passed
    })
}

mod features;
mod in_process;
mod part;
mod step;
#[cfg(test)]
mod tests;
mod windows_lint;
