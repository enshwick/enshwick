//! `cargo xtask check`: the compiling checks a change needs before it
//! merges; the git hooks run only the checks that compile nothing.
//!
//! Nothing runs when no package changed since the branch left `origin/main`.
//! Otherwise the tree's own rules run ([`crate::check`]), then clippy and
//! rustdoc over the workspace, where unchanged packages are already fresh,
//! and nextest and the doctests run for the changed packages and every
//! package that depends on them; a local check leaves the widest part of
//! that to CI ([`affected::Lane`]). Off Windows the workspace is also
//! linted for Windows through `cargo xwin`, beside the host checks.
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
use std::time::Instant;

use crate::affected::{self, Lane, Package, Scope};
use crate::commands::{fuzz, header, wine};
use crate::{FACADE, cargo, packages, say, tool_path, workspace_root};

pub(crate) const WINDOWS_TARGET: &str = "x86_64-pc-windows-msvc";
/// Turned on by name: most of the API crate's test targets need its fake
/// loader, and without the feature cargo skips them without a word; the
/// API's unstable calls, its generated typed API and the compat task are
/// linted and tested with the rest.
const TEST_FEATURE: &str = "enshwick/test,enshwick/unstable,enshwick/typed,xtask/compat";

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
    /// Lists, one a line, the packages whose Windows tests the change
    /// reaches, and checks nothing.
    #[arg(long)]
    list_windows_tests: bool,
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
    if parts.contains(&Part::Fuzz) {
        let fuzzed = fuzz_part(changed.as_deref(), args.all);
        if !fuzzed || parts.len() == 1 {
            return Ok(fuzzed);
        }
    }
    let packages = packages()?;
    let root = workspace_root();
    if parts.contains(&Part::Host) && !args.list_windows_tests {
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
    if args.list_windows_tests {
        // One name a line: the packages CI tests on a Windows runner.
        for name in &windows_tests {
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
    if parts.contains(&Part::Trybuild) && !touches(&scope, FACADE) {
        say(
            &if matches!(&scope, Scope::Some { tested, .. } if tested.contains(FACADE)) {
                format!("the trybuild test is left to CI: only {FACADE}'s tests run here")
            } else if reaches(&scope, FACADE) {
                format!("the trybuild test is left to CI: {FACADE} is only built here")
            } else {
                format!("the trybuild test is out of scope: the change does not reach {FACADE}")
            },
        );
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

/// Whether `scope` checks the code of `package`.
fn touches(scope: &Scope, package: &str) -> bool {
    match scope {
        Scope::All => true,
        Scope::Some { code, .. } => code.iter().any(|name| name == package),
    }
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

/// A part of the check, so CI can run the parts as parallel jobs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub(crate) enum Part {
    /// fmt with `--all`, clippy, nextest and cargo-deny for the host, and the
    /// repository's own checks.
    Host,
    /// The doctests and the workspace rustdoc, for the host.
    Docs,
    /// The API crate's stable-surface checks ([`stable_api_steps`]): a
    /// build of its own without `unstable`, which shares nothing with the
    /// docs part's, so CI runs it beside that part.
    StableApi,
    /// clippy for the Windows target through cargo-xwin, off Windows.
    WindowsLint,
    /// The tests of the changed packages that run only on Windows, under
    /// Wine on an amd64 Linux host.
    WindowsTests,
    /// Short runs of the fuzz targets the change reaches ([`fuzz`]); only
    /// when named, since it needs the nightly compiler.
    Fuzz,
    /// The trybuild test ([`TRYBUILD`]), when the API crate's tests are in
    /// the change's scope; a run that finds no such test fails, so a moved
    /// test does not go untested. The host part never runs it.
    Trybuild,
    /// cargo-semver-checks for the packages `--semver` names.
    Semver,
}

impl Part {
    /// The parts that run when none is named: the host check, the docs, the
    /// stable surface, the Windows lint, the trybuild test and semver-checks,
    /// and the Windows tests with `--all` or `--windows`.
    fn every(windows_tests: bool) -> Vec<Self> {
        let mut every = vec![
            Self::Host,
            Self::Docs,
            Self::StableApi,
            Self::WindowsLint,
            Self::Trybuild,
            Self::Semver,
        ];
        if windows_tests {
            every.push(Self::WindowsTests);
        }
        every
    }
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

/// One command line: cargo, or another tool of the checks.
#[derive(Debug)]
pub(crate) struct Step {
    program: Option<String>,
    args: Vec<String>,
    env: Vec<(&'static str, String)>,
}

impl Step {
    pub(crate) fn new(words: &[&str]) -> Self {
        Self {
            program: None,
            args: words.iter().map(|w| (*w).to_owned()).collect(),
            env: Vec::new(),
        }
    }

    /// A step that runs `program` instead of cargo.
    pub(crate) fn tool(program: &'static str, words: &[&str]) -> Self {
        Self {
            program: Some(program.to_owned()),
            ..Self::new(words)
        }
    }

    /// A step that runs this xtask binary again, without a cargo start and
    /// its rebuild check of the binary that runs.
    fn xtask(words: &[&str]) -> Self {
        match std::env::current_exe() {
            Ok(exe) => Self {
                program: Some(exe.display().to_string()),
                ..Self::new(words)
            },
            Err(_) => Self::new(&["xtask"]).then(words),
        }
    }

    fn then_owned(mut self, words: &[String]) -> Self {
        self.args.extend_from_slice(words);
        self
    }

    pub(crate) fn then(mut self, words: &[&str]) -> Self {
        self.args.extend(words.iter().map(|w| (*w).to_owned()));
        self
    }

    fn env(mut self, name: &'static str, value: impl Into<String>) -> Self {
        self.env.push((name, value.into()));
        self
    }

    fn line(&self) -> String {
        format!(
            "{} {}",
            self.program.as_deref().unwrap_or("cargo"),
            self.args.join(" ")
        )
    }

    /// Runs the step with its output shown, or captured when `quiet` (for a
    /// step running beside others) and shown only when it fails.
    pub(crate) fn run(&self, quiet: bool) -> bool {
        let started = Instant::now();
        let mut command = Command::new(self.program.clone().unwrap_or_else(cargo));
        command
            .args(&self.args)
            .env("PATH", tool_path())
            .envs(self.env.iter().map(|(name, value)| (name, value)));
        let (ok, output) = if quiet {
            match command.output() {
                Ok(output) => (
                    output.status.success(),
                    [output.stdout, output.stderr].concat(),
                ),
                Err(error) => (false, error.to_string().into_bytes()),
            }
        } else {
            say(&format!("> {}", self.line()));
            match command.status() {
                Ok(status) => (status.success(), Vec::new()),
                Err(error) => (false, error.to_string().into_bytes()),
            }
        };
        let seconds = started.elapsed().as_secs_f32();
        if ok {
            say(&format!("ok in {seconds:.1} s: {}", self.line()));
        } else {
            let _shown = std::io::stderr().write_all(&output);
            say(&format!("FAILED in {seconds:.1} s: {}", self.line()));
        }
        ok
    }
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
/// its lints when its code changed, its examples and its rustdoc whenever
/// it changed. `stable` names the crates and features ([`stable_api`]).
fn stable_api_steps(code: bool, stable: &[String]) -> Vec<Step> {
    let mut steps = Vec::new();
    if code {
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
    steps.push(doctests);
    // A link into an unstable module breaks only this build, and the
    // crate's own rustdoc takes seconds once the examples above built
    // its dependencies.
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
/// and in the full suite. It takes seconds once clippy built the workspace.
const fn workspace_rustdoc(code: bool, full: bool) -> bool {
    code || full
}

/// The ast-grep rules' own tests, the rules over the repository
/// ([`crate::check::rules`]), then the dependencies no crate uses.
fn source_checks() -> bool {
    Step::tool("ast-grep", &["test", "--skip-snapshot-tests"]).run(false)
        && crate::check::rules::scan(&workspace_root())
        && Step::tool("cargo-machete", &[]).run(false)
}

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
            args.extend(["--exclude".to_owned(), package.name.clone()]);
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
        // CI runs each part in a job of its own, so the host part never
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
    if parts.contains(&Part::Trybuild) && touches(scope, FACADE) {
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
    // The API crate's examples, the book's among them, change with its
    // documents as well as with its code.
    let api_docs = match scope {
        Scope::All => true,
        Scope::Some { docs, .. } => docs.iter().any(|name| name == FACADE),
    };
    if parts.contains(&Part::StableApi) && (touches(scope, FACADE) || api_docs) {
        steps.extend(stable_api_steps(touches(scope, FACADE), stable));
    }
    if run_docs && workspace_rustdoc(code, full) {
        // `code` alone: a package whose version alone moved needs no docs.
        steps.push(host(&["doc", "--no-deps"]).env("RUSTDOCFLAGS", "-D warnings"));
        steps.push(doc_links_step());
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
                "running only the tests of {}: CI and the merge queue run their examples{}",
                names.join(", "),
                if tested.contains(FACADE) {
                    format!(" and {FACADE}'s stable surface")
                } else {
                    String::new()
                }
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
    let (flags, tests, doctests) = plan(scope, packages, host_ok);
    let workspace = host_args(packages, on_windows);
    let stable = stable_api(packages);
    let host_steps = host_steps(
        scope,
        (&workspace, &stable),
        flags,
        tests.as_deref(),
        &doctests,
        full,
        parts,
    );
    // Off Windows the whole workspace is also linted for Windows, where the
    // `cfg(windows)` code builds.
    let windows = (!on_windows && flags.0 && parts.contains(&Part::WindowsLint)).then(|| {
        Step::new(&[
            "xwin",
            "clippy",
            "--target",
            WINDOWS_TARGET,
            "--workspace",
            "--features",
            TEST_FEATURE,
            "--all-targets",
            "--",
            "-D",
            "warnings",
        ])
    });

    if let Some(step) = &windows {
        say(&format!("> {} (beside the host checks)", step.line()));
    }
    thread::scope(|scope| {
        let windows = windows.as_ref().map(|step| scope.spawn(|| step.run(true)));
        // The first failing host step ends the host checks.
        let host_passed = host_steps.iter().all(|step| step.run(false));
        let windows_passed = windows.is_none_or(|handle| handle.join().unwrap_or(false));
        host_passed && windows_passed
    })
}

#[cfg(test)]
mod tests;
