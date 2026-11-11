//! Repository tasks, run as `cargo xtask <task>`.
//!
//! `cargo xtask check` runs the compiling checks a change needs before it
//! merges; the git hooks run only the checks that compile nothing. Nothing
//! runs when no package changed since the branch left `origin/main`.
//! Otherwise clippy, the doctests and rustdoc run over the workspace, where unchanged
//! packages are already fresh, and nextest runs the tests of the changed
//! packages and of every package that depends on them. Off Windows the
//! workspace is also linted for Windows through `cargo xwin`, beside the host
//! checks. `cargo xtask check --windows` also runs the Windows tests of the
//! crates marked `windows-tests` or `windows-only`, under Wine on an amd64
//! Linux host. `cargo xtask check --all` runs the full suite, the Windows
//! tests included where they can run, but no fuzzing. `cargo xtask check
//! --part fuzz` runs the fuzz targets the change reaches for a short while,
//! or with `--all` every target ([`fuzz`]).
//!
//! `cargo xtask dist` writes `enshwick.zip`, the loader that players unpack
//! into the game folder ([`dist`]), and `cargo xtask notes` the notes of its
//! alpha release ([`notes`]). `cargo xtask tag-lines` moves the install and
//! dependency lines of the Markdown to the workspace version's tag, which the
//! check holds ([`tag_lines`]). `cargo xtask header` rewrites the ABI's
//! C header, which the check holds to the ABI module and its golden layout
//! ([`header`]). `cargo xtask alias-examples` writes the hidden line that
//! names an internal crate `enshwick` into its doc examples, which the
//! check holds with their copies in the facade ([`lower_examples`]).
//!
//! `cargo xtask compat` reports what works on a game build. It links the
//! loader, so it is the binary `xtask-compat` of this package, built with
//! the `compat` feature; `cargo xtask compat` builds and runs it, and
//! `cargo run -p xtask --bin xtask-compat --features compat -- <exe>` runs it
//! directly.

#![forbid(unsafe_code)]

mod affected;
mod book;
mod book_links;
mod book_prose;
mod crate_rules;
mod dist;
mod fuzz;
mod header;
mod lower_examples;
mod notes;
mod ranges;
mod reports;
mod rules;
mod tag_lines;
mod wine;

use std::collections::BTreeSet;
use std::io::Write as _;
use std::process::{Command, ExitCode};
use std::thread;
use std::time::Instant;

use affected::{Package, Scope};

const USAGE: &str = "usage: cargo xtask check [--all] [--windows] [--base <git ref>] [--part host|docs|windows-lint|windows-tests|fuzz]... [--list-windows-tests] (--all fuzzes only with --part fuzz) | dist [--out <dir>] | notes [--out <file>] [--known-issues <text>] | tag-lines | header | alias-examples | host-args | compat <exe> [--mods <path>...] [--report <file>] [--patterns] [--data <file>] | compat refresh (ENSH_EXES lists the executables)";
const WINDOWS_TARGET: &str = "x86_64-pc-windows-msvc";
/// Turned on by name: most of the API crate's test targets need its fake
/// loader, and without the feature cargo skips them without a word; the
/// API's unstable calls, its generated typed API and the compat task are
/// linted and tested with the rest.
const TEST_FEATURE: &str = "enshwick/test,enshwick/unstable,enshwick/typed,xtask/compat";
/// The API crate, its core and its fake loader, whose modules and examples
/// it re-exports, without `unstable`: the workspace turns the feature on for
/// the runtimes, so only a build of their own shows that the stable surface,
/// its docs and its examples stand without the unstable one.
const STABLE_API: [&str; 8] = [
    "--package",
    "enshwick",
    "--package",
    "enshwick-core",
    "--package",
    "enshwick-fake",
    "--features",
    "enshwick/test",
];

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(("compat", rest)) = args.split_first().map(|(task, rest)| (task.as_str(), rest)) {
        return compat_task(rest);
    }
    match run(&args) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(error) => {
            let _shown = writeln!(std::io::stderr(), "xtask: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[String]) -> Result<bool, String> {
    let (task, rest) = args.split_first().ok_or(USAGE)?;
    match task.as_str() {
        "check" => {}
        "dist" => return dist::run(rest),
        "notes" => return notes::run(rest),
        "tag-lines" => return tag_lines::run(rest),
        "header" if rest.is_empty() => return header::run(&workspace_root()),
        "alias-examples" if rest.is_empty() => {
            return lower_examples::run(&workspace_root(), &packages()?);
        }
        "host-args" if rest.is_empty() => {
            // One argument a line, for CI runs that test the host as the check does.
            for arg in host_args(&packages()?, cfg!(windows)) {
                let _shown = writeln!(std::io::stdout(), "{arg}");
            }
            return Ok(true);
        }
        _ => return Err(USAGE.to_owned()),
    }
    let mut all = false;
    let mut windows = false;
    let mut list = false;
    let mut parts = Vec::new();
    let mut base = "origin/main".to_owned();
    let mut rest = rest.iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--all" => all = true,
            "--windows" => windows = true,
            "--list-windows-tests" => list = true,
            "--base" => base.clone_from(rest.next().ok_or(USAGE)?),
            "--part" => parts.push(Part::parse(rest.next().ok_or(USAGE)?)?),
            _ => return Err(USAGE.to_owned()),
        }
    }
    let parts = if parts.is_empty() {
        Part::every(all || windows)
    } else {
        parts
    };
    if parts.contains(&Part::Fuzz) {
        let fuzzed = fuzz_part(&base, all);
        if !fuzzed || parts.len() == 1 {
            return Ok(fuzzed);
        }
    }
    let packages = packages()?;
    let root = workspace_root();
    let problems = tree_problems(&root, &packages)?;
    let changed = changed_files(&base);
    if parts.contains(&Part::Host) && !list {
        for problem in &problems {
            say(problem);
        }
        if !problems.is_empty() {
            return Ok(false);
        }
    }
    let scope = if all {
        Scope::All
    } else if let Some(files) = &changed {
        affected::scope(files, &packages)
    } else {
        say(&format!("no merge base with {base}; checking everything"));
        Scope::All
    };
    let windows_tests: Vec<&str> = packages
        .iter()
        .filter(|p| p.windows_tests)
        .filter(|p| match &scope {
            Scope::All => true,
            Scope::Some { code, docs } => code.contains(&p.name) || docs.contains(&p.name),
        })
        .map(|p| p.name.as_str())
        .collect();
    if list {
        // One name a line: the packages CI tests on a Windows runner.
        for name in &windows_tests {
            let _shown = writeln!(std::io::stdout(), "{name}");
        }
        return Ok(true);
    }
    if parts.contains(&Part::Host) && touches(&scope, header::PACKAGE) {
        let problems = header::check(&root);
        for problem in &problems {
            say(&format!("C header: {problem}"));
        }
        if !problems.is_empty() {
            return Ok(false);
        }
    }
    let checked = check(&scope, &packages, all, &parts);
    if !(checked && parts.contains(&Part::WindowsTests)) {
        return Ok(checked);
    }
    if windows_tests.is_empty() {
        say("no package with Windows tests changed; no tests under Wine");
        return Ok(true);
    }
    Ok(wine::run(&windows_tests, windows))
}

/// The problems the checks that compile nothing find in the tree: manifest
/// ranges, the book, its links and sentence lengths, the tag lines, the
/// crate rules, the internal crates' examples and the seals of the
/// compatibility reports.
fn tree_problems(root: &std::path::Path, packages: &[Package]) -> Result<Vec<String>, String> {
    let mut problems: Vec<String> = ranges::check(root, packages)
        .into_iter()
        .map(|problem| format!("manifest range: {problem}"))
        .collect();
    problems.extend(
        book::check(root)
            .into_iter()
            .map(|problem| format!("book: {problem}")),
    );
    problems.extend(
        book_prose::check(root)
            .into_iter()
            .map(|problem| format!("book prose: {problem}")),
    );
    let mods = book_links::mods_clone(root).map_err(|why| format!("book links: {why}"))?;
    let links = book_links::check(root, mods.as_deref());
    if let Some(note) = &links.skipped {
        say(note);
    }
    problems.extend(
        links
            .problems
            .into_iter()
            .map(|problem| format!("book link: {problem}")),
    );
    problems.extend(
        tag_lines::check(root, packages)
            .into_iter()
            .map(|problem| format!("tag line: {problem}")),
    );
    problems.extend(
        crate_rules::check()?
            .into_iter()
            .map(|problem| format!("crate rule: {problem}")),
    );
    problems.extend(
        lower_examples::check(root, packages)
            .into_iter()
            .map(|problem| format!("internal example: {problem}")),
    );
    problems.extend(
        reports::stale(root, &[])
            .into_iter()
            .map(|problem| format!("compatibility report: {problem}")),
    );
    Ok(problems)
}

/// The workspace root, two levels above this crate.
fn workspace_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Whether `scope` checks the code of `package`.
fn touches(scope: &Scope, package: &str) -> bool {
    match scope {
        Scope::All => true,
        Scope::Some { code, .. } => code.iter().any(|name| name == package),
    }
}

/// The fuzz part: the targets the change since `base` reaches, or every
/// target in the full suite or without a merge base; whether none crashed.
fn fuzz_part(base: &str, all: bool) -> bool {
    let targets = match changed_files(base) {
        Some(files) if !all => fuzz::reached(&files),
        _ => fuzz::all(),
    };
    fuzz::run(&targets)
}

/// A part of the check, so CI can run the parts as parallel jobs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Part {
    /// fmt with `--all`, clippy, nextest and cargo-deny for the host, and the
    /// repository's own checks.
    Host,
    /// The doctests, the API crate's stable-surface checks and the workspace
    /// rustdoc, for the host.
    Docs,
    /// clippy for the Windows target through cargo-xwin, off Windows.
    WindowsLint,
    /// The tests of the changed packages that run only on Windows, under
    /// Wine on an amd64 Linux host.
    WindowsTests,
    /// Short runs of the fuzz targets the change reaches ([`fuzz`]); only
    /// when named, since it needs the nightly compiler.
    Fuzz,
}

impl Part {
    /// The parts that run when none is named: the host check, the docs and
    /// the Windows lint, and the Windows tests with `--all` or `--windows`.
    fn every(windows_tests: bool) -> Vec<Self> {
        let mut every = vec![Self::Host, Self::Docs, Self::WindowsLint];
        if windows_tests {
            every.push(Self::WindowsTests);
        }
        every
    }

    fn parse(name: &str) -> Result<Self, String> {
        match name {
            "host" => Ok(Self::Host),
            "docs" => Ok(Self::Docs),
            "windows-lint" => Ok(Self::WindowsLint),
            "windows-tests" => Ok(Self::WindowsTests),
            "fuzz" => Ok(Self::Fuzz),
            _ => Err(format!("unknown part {name}; {USAGE}")),
        }
    }
}

/// The kinds cargo metadata gives a library target.
const LIB_KINDS: [&str; 6] = ["lib", "rlib", "dylib", "cdylib", "staticlib", "proc-macro"];

/// The workspace packages, from `cargo metadata`.
fn packages() -> Result<Vec<Package>, String> {
    let output = Command::new(cargo())
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .output()
        .map_err(|error| format!("running cargo metadata: {error}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned());
    }
    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("reading cargo metadata: {error}"))?;
    let root = metadata["workspace_root"]
        .as_str()
        .unwrap_or_default()
        .replace('\\', "/");
    let list = metadata["packages"]
        .as_array()
        .ok_or("cargo metadata lists no packages")?;
    let names: BTreeSet<&str> = list.iter().filter_map(|p| p["name"].as_str()).collect();
    Ok(list
        .iter()
        .map(|package| {
            let manifest = package["manifest_path"]
                .as_str()
                .unwrap_or_default()
                .replace('\\', "/");
            let dir = manifest
                .strip_prefix(&format!("{root}/"))
                .and_then(|path| path.strip_suffix("/Cargo.toml"))
                .unwrap_or_default()
                .to_owned();
            let deps = package["dependencies"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|dep| dep["name"].as_str())
                .filter(|name| names.contains(name))
                .map(str::to_owned)
                .collect();
            let lib = package["targets"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|target| {
                    target["kind"].as_array().is_some_and(|kinds| {
                        kinds
                            .iter()
                            .any(|kind| LIB_KINDS.contains(&kind.as_str().unwrap_or_default()))
                    })
                })
                .and_then(|target| target["name"].as_str())
                .map(|name| name.replace('-', "_"));
            Package {
                name: package["name"].as_str().unwrap_or_default().to_owned(),
                version: package["version"].as_str().unwrap_or_default().to_owned(),
                dir,
                deps,
                lib,
                windows_only: package["metadata"]["xtask"]["windows-only"].as_bool() == Some(true),
                windows_tests: ["windows-tests", "windows-only"]
                    .iter()
                    .any(|key| package["metadata"]["xtask"][key].as_bool() == Some(true)),
            }
        })
        .collect())
}

/// Files changed between the merge base with `base` and the working tree,
/// untracked ones included; `None` without a merge base.
fn changed_files(base: &str) -> Option<Vec<String>> {
    let merge_base = git(&["merge-base", "HEAD", base])?;
    let mut files: Vec<String> = git(&["diff", "--name-only", merge_base.trim()])?
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

fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

/// `cargo xtask compat` runs `xtask-compat`, the binary built with the
/// loader, which plain `cargo xtask` leaves out to stay quick to build.
fn compat_task(args: &[String]) -> ExitCode {
    let status = Command::new(cargo())
        .args([
            "run",
            "--quiet",
            "--package",
            "xtask",
            "--bin",
            "xtask-compat",
            "--features",
            "compat",
            "--",
        ])
        .args(args)
        .status();
    match status {
        Ok(status) => status
            .code()
            .and_then(|code| u8::try_from(code).ok())
            .map_or(ExitCode::FAILURE, ExitCode::from),
        Err(error) => {
            let _shown = writeln!(std::io::stderr(), "xtask: running cargo: {error}");
            ExitCode::FAILURE
        }
    }
}

fn cargo() -> String {
    std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned())
}

/// `PATH` for the tools the checks run, with LLVM's default Windows folder
/// last: its installer leaves the folder off `PATH` unless asked, and the
/// header check and the C example need its clang.
pub(crate) fn tool_path() -> std::ffi::OsString {
    let path = std::env::var_os("PATH").unwrap_or_default();
    let llvm = std::env::var_os("ProgramFiles")
        .filter(|_| cfg!(windows))
        .map(|programs| std::path::Path::new(&programs).join("LLVM").join("bin"));
    std::env::join_paths(std::env::split_paths(&path).chain(llvm)).unwrap_or(path)
}

/// One command line: cargo, or another tool of the checks.
#[derive(Debug)]
struct Step {
    program: Option<&'static str>,
    args: Vec<String>,
    env: Vec<(&'static str, String)>,
}

impl Step {
    fn new(words: &[&str]) -> Self {
        Self {
            program: None,
            args: words.iter().map(|w| (*w).to_owned()).collect(),
            env: Vec::new(),
        }
    }

    /// A step that runs `program` instead of cargo.
    fn tool(program: &'static str, words: &[&str]) -> Self {
        Self {
            program: Some(program),
            ..Self::new(words)
        }
    }

    fn then_owned(mut self, words: &[String]) -> Self {
        self.args.extend_from_slice(words);
        self
    }

    fn then(mut self, words: &[&str]) -> Self {
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
            self.program.unwrap_or("cargo"),
            self.args.join(" ")
        )
    }

    /// Runs the step with its output shown, or captured when `quiet` (for a
    /// step running beside others) and shown only when it fails.
    fn run(&self, quiet: bool) -> bool {
        let started = Instant::now();
        let mut command = Command::new(self.program.map_or_else(cargo, str::to_owned));
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

fn say(text: &str) {
    let _shown = writeln!(std::io::stderr(), "xtask: {text}");
}

/// The checks of the API crate's stable surface, built without `unstable`:
/// its lints when its code changed, its examples and its rustdoc whenever
/// it changed.
fn stable_api_steps(code: bool) -> Vec<Step> {
    let mut steps = Vec::new();
    if code {
        steps.push(
            Step::new(&["clippy", "--all-targets"])
                .then(&STABLE_API)
                .then(&["--", "-D", "warnings"]),
        );
    }
    // The script binding, the rates and item data are public only with
    // `unstable`, so their examples fail to build without it. They are
    // marked `standalone_crate` and skipped here: one example that fails
    // in the merged build makes rustdoc build the rest one by one, which
    // takes minutes; merged they share one binary and take seconds. The
    // test names carry the source path with the host's separator.
    let mut doctests = Step::new(&["test", "--doc"])
        .then(&STABLE_API)
        .then(&["--"]);
    for module in ["src/script", "src/rates.rs", "src/item_data.rs"] {
        let windows = module.replace('/', "\\");
        doctests = doctests.then(&["--skip", module, "--skip", &windows]);
    }
    steps.push(doctests);
    // A link into an unstable module breaks only this build, and the
    // crate's own rustdoc takes seconds once the examples above built
    // its dependencies.
    steps.push(
        Step::new(&["doc", "--no-deps"])
            .then(&STABLE_API)
            .env("RUSTDOCFLAGS", "-D warnings"),
    );
    steps
}

/// Whether the workspace's rustdoc runs: whenever code changed, since a
/// change to one crate breaks links in the docs of the crates that use it,
/// and in the full suite. It takes seconds once clippy built the workspace.
const fn workspace_rustdoc(code: bool, full: bool) -> bool {
    code || full
}

/// The ast-grep rules' own tests, the rules over the repository ([`rules`]),
/// then the dependencies no crate uses.
fn source_checks() -> bool {
    Step::tool("ast-grep", &["test", "--skip-snapshot-tests"]).run(false)
        && rules::scan(&workspace_root())
        && Step::tool("cargo-machete", &[]).run(false)
}

/// The cargo arguments every host build and test of the workspace takes.
fn host_args(packages: &[Package], on_windows: bool) -> Vec<String> {
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
/// every part runs them: `code` and `docs` say what changed, and `tests` is
/// the nextest filter of the changed packages (`None`: every test).
fn host_steps(
    scope: &Scope,
    workspace: &[String],
    (code, docs): (bool, bool),
    tests: Option<&str>,
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
    if code && run_host {
        steps.push(host(&["clippy", "--all-targets"]).then(&["--", "-D", "warnings"]));
        match tests {
            Some("") => {}
            Some(filter) => {
                steps.push(host(&["nextest", "run", "--no-tests=pass"]).then(&["-E", filter]));
            }
            None => steps.push(host(&["nextest", "run", "--no-tests=pass"])),
        }
    }
    if docs && run_docs {
        steps.push(host(&["test", "--doc"]));
    }
    // The API crate's examples, the book's among them, change with its
    // documents as well as with its code.
    let api_docs = match scope {
        Scope::All => true,
        Scope::Some { docs, .. } => docs.iter().any(|name| name == "enshwick"),
    };
    if run_docs && (touches(scope, "enshwick") || api_docs) {
        steps.extend(stable_api_steps(touches(scope, "enshwick")));
    }
    if run_docs && workspace_rustdoc(code, full) {
        steps.push(host(&["doc", "--no-deps"]).env("RUSTDOCFLAGS", "-D warnings"));
    }
    if full && run_host {
        steps.push(Step::new(&["deny", "check"]));
    }
    steps
}

/// Runs the checks of `scope`. Every build selects the whole workspace, so
/// features unify the same way on every run and nothing builds twice; only
/// the tests run are narrowed to the changed packages. Host steps run one
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
    let (code, docs, tests) = match scope {
        Scope::All => (true, true, None),
        Scope::Some { code, docs } => {
            if code.is_empty() && docs.is_empty() {
                say("no package changed; nothing to check");
                return true;
            }
            let names: Vec<&str> = code.iter().chain(docs).map(String::as_str).collect();
            say(&format!("checking {}", names.join(", ")));
            let host: Vec<&String> = code.iter().filter(|name| host_ok(name)).collect();
            let filter = host
                .iter()
                .map(|name| format!("package({name})"))
                .collect::<Vec<_>>()
                .join(" | ");
            (
                !code.is_empty(),
                !host.is_empty() || !docs.is_empty(),
                Some(filter),
            )
        }
    };
    if matches!(scope, Scope::All) {
        say("checking the whole workspace");
    }
    let workspace = host_args(packages, on_windows);
    let host_steps = host_steps(
        scope,
        &workspace,
        (code, docs),
        tests.as_deref(),
        full,
        parts,
    );
    // Off Windows the whole workspace is also linted for Windows, where the
    // `cfg(windows)` code builds.
    let windows = (!on_windows && code && parts.contains(&Part::WindowsLint)).then(|| {
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
mod tests {
    use super::*;

    #[test]
    fn the_docs_part_parses_and_runs_when_no_part_is_named() {
        assert_eq!(Part::parse("docs"), Ok(Part::Docs));
        assert!(Part::every(false).contains(&Part::Docs));
        assert!(Part::every(true).contains(&Part::Docs));
        assert!(Part::parse("doc").is_err());
    }

    #[test]
    fn the_loader_names_the_facade_its_tests_build_mods_on() {
        let packages = packages().unwrap();
        let loader = packages
            .iter()
            .find(|p| p.name == "enshwick-loader")
            .unwrap();
        assert!(
            loader.deps.iter().any(|dep| dep == "enshwick"),
            "{:?}",
            loader.deps
        );
    }

    fn planned(parts: &[Part]) -> Vec<String> {
        host_steps(&Scope::All, &[], (true, true), None, true, parts)
            .iter()
            .map(Step::line)
            .collect()
    }

    #[test]
    fn the_host_and_docs_parts_split_the_steps_a_check_of_every_part_runs_in_order() {
        let (host, docs) = (planned(&[Part::Host]), planned(&[Part::Docs]));
        let starts = |steps: &[String], with: &str| steps.iter().any(|s| s.starts_with(with));
        assert!(starts(&host, "cargo nextest") && starts(&host, "cargo deny"));
        assert!(!starts(&host, "cargo test --doc") && !starts(&host, "cargo doc"));
        assert!(starts(&docs, "cargo test --doc") && starts(&docs, "cargo doc"));
        assert!(!starts(&docs, "cargo nextest") && !starts(&docs, "cargo fmt"));
        let every = planned(&Part::every(false));
        let order = |steps: &[String]| -> Vec<usize> {
            steps
                .iter()
                .map(|step| every.iter().position(|s| s == step).unwrap())
                .collect()
        };
        assert_eq!(every.len(), host.len() + docs.len());
        assert!(order(&host).is_sorted() && order(&docs).is_sorted());
    }

    #[test]
    fn a_change_to_the_api_docs_alone_plans_the_stable_steps_in_the_docs_part() {
        let docs_of = |name: &str| Scope::Some {
            code: BTreeSet::new(),
            docs: [name.to_owned()].into(),
        };
        let stable = |scope: &Scope| {
            host_steps(scope, &[], (false, true), Some(""), false, &[Part::Docs])
                .iter()
                .any(|step| step.args.ends_with(&STABLE_API.map(str::to_owned)))
        };
        assert!(stable(&docs_of("enshwick")));
        assert!(!stable(&docs_of("enshwick-luau")));
    }

    fn runs_stable_rustdoc(steps: &[Step]) -> bool {
        steps.iter().any(|step| {
            step.args
                .starts_with(&["doc".to_owned(), "--no-deps".to_owned()])
                && step.args.ends_with(&STABLE_API.map(str::to_owned))
                && step
                    .env
                    .iter()
                    .any(|(name, value)| *name == "RUSTDOCFLAGS" && value == "-D warnings")
        })
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
        assert!(runs_stable_rustdoc(&stable_api_steps(true)));
    }

    #[test]
    fn a_change_to_the_book_alone_builds_the_stable_rustdoc() {
        assert!(runs_stable_rustdoc(&stable_api_steps(false)));
    }

    #[test]
    fn a_change_to_code_builds_the_workspace_rustdoc_outside_the_full_suite() {
        assert!(workspace_rustdoc(true, false));
        assert!(workspace_rustdoc(false, true));
        assert!(!workspace_rustdoc(false, false));
    }

    #[test]
    fn the_stable_doctests_skip_unstable_modules_under_either_path_separator() {
        let steps = stable_api_steps(false);
        let skipped = |name: &str| {
            steps.iter().any(|step| {
                step.args
                    .windows(2)
                    .any(|pair| pair[0] == "--skip" && name.contains(pair[1].as_str()))
            })
        };
        for file in ["src/script/mod.rs", "src/rates.rs", "src/item_data.rs"] {
            let name = format!("crates/internal/core/{file} - item (line 1)");
            assert!(skipped(&name), "{name}");
            assert!(skipped(&name.replace('/', "\\")), "{name}");
        }
        assert!(!skipped("crates\\enshwick\\src\\lib.rs - item (line 1)"));
    }
}
