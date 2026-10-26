//! Repository tasks, run as `cargo xtask <task>`.
//!
//! `cargo xtask check` runs the compiling checks a change needs before it
//! merges; the git hooks run only the checks that compile nothing. Nothing
//! runs when no package changed since the branch left `origin/main`.
//! Otherwise clippy and the doctests run over the workspace, where unchanged
//! packages are already fresh, and nextest runs the tests of the changed
//! packages and of every package that depends on them. Off Windows the
//! workspace is also linted for Windows through `cargo xwin`, beside the host
//! checks. `cargo xtask check --windows` also runs the Windows tests of the
//! crates marked `windows-tests` or `windows-only`, under Wine off Windows. `cargo xtask check
//! --all` runs the full suite, the Windows tests included when Docker runs.
//!
//! `cargo xtask bundle` writes the dev bundle testers unpack into the game
//! folder ([`bundle`]).

mod affected;
mod book;
mod bundle;
mod ranges;
mod wine;

use std::collections::BTreeSet;
use std::io::Write as _;
use std::process::{Command, ExitCode};
use std::thread;
use std::time::Instant;

use affected::{Package, Scope};

const USAGE: &str = "usage: cargo xtask check [--all] [--windows] [--base <git ref>] | bundle [--out <dir>] [--local]";
const WINDOWS_TARGET: &str = "x86_64-pc-windows-msvc";
/// Turned on by name: most of the API crate's test targets need its fake
/// loader, and without the feature cargo skips them without a word.
const TEST_FEATURE: &str = "enshwick/test";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
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
        "bundle" => return bundle::run(rest),
        "wine-run" => return wine::run_one(rest),
        _ => return Err(USAGE.to_owned()),
    }
    let mut all = false;
    let mut windows = false;
    let mut base = "origin/main".to_owned();
    let mut rest = rest.iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--all" => all = true,
            "--windows" => windows = true,
            "--base" => base.clone_from(rest.next().ok_or(USAGE)?),
            _ => return Err(USAGE.to_owned()),
        }
    }
    let packages = packages()?;
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut problems: Vec<String> = ranges::check(&root, &packages)
        .into_iter()
        .map(|problem| format!("manifest range: {problem}"))
        .collect();
    problems.extend(
        book::check(&root)
            .into_iter()
            .map(|problem| format!("book: {problem}")),
    );
    for problem in &problems {
        say(problem);
    }
    if !problems.is_empty() {
        return Ok(false);
    }
    let scope = if all {
        Scope::All
    } else if let Some(files) = changed_files(&base) {
        affected::scope(&files, &packages)
    } else {
        say(&format!("no merge base with {base}; checking everything"));
        Scope::All
    };
    let checked = check(&scope, &packages, all);
    if !(checked && (all || windows)) {
        return Ok(checked);
    }
    let windows_tests: Vec<&str> = packages
        .iter()
        .filter(|p| p.windows_tests)
        .filter(|p| match &scope {
            Scope::All => true,
            Scope::Some { code, docs } => code.contains(&p.name) || docs.contains(&p.name),
        })
        .map(|p| p.name.as_str())
        .collect();
    if windows_tests.is_empty() {
        say("no package with Windows tests changed; no tests under Wine");
        return Ok(true);
    }
    Ok(wine::run(&windows_tests, windows))
}

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
            Package {
                name: package["name"].as_str().unwrap_or_default().to_owned(),
                version: package["version"].as_str().unwrap_or_default().to_owned(),
                dir,
                deps,
                windows_only: package["metadata"]["enshwick"]["windows-only"].as_bool()
                    == Some(true),
                windows_tests: ["windows-tests", "windows-only"]
                    .iter()
                    .any(|key| package["metadata"]["enshwick"][key].as_bool() == Some(true)),
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
        .chain(git(&["ls-files", "--others", "--exclude-standard"])?.lines())
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

fn cargo() -> String {
    std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned())
}

/// One cargo command line.
#[derive(Debug)]
struct Step {
    args: Vec<String>,
    env: Vec<(&'static str, String)>,
}

impl Step {
    fn new(words: &[&str]) -> Self {
        Self {
            args: words.iter().map(|w| (*w).to_owned()).collect(),
            env: Vec::new(),
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
        format!("cargo {}", self.args.join(" "))
    }

    /// Runs the step with its output shown, or captured when `quiet` (for a
    /// step running beside others) and shown only when it fails.
    fn run(&self, quiet: bool) -> bool {
        let started = Instant::now();
        let mut command = Command::new(cargo());
        command
            .args(&self.args)
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

/// Runs the checks of `scope`. Every build selects the whole workspace, so
/// features unify the same way on every run and nothing builds twice; only
/// the tests run are narrowed to the changed packages. Host steps run one
/// after another (they share the build lock); the Windows lint runs beside
/// them.
fn check(scope: &Scope, packages: &[Package], full: bool) -> bool {
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
    let mut workspace = vec![
        "--workspace".to_owned(),
        "--features".to_owned(),
        TEST_FEATURE.to_owned(),
    ];
    // Off Windows the crates that build only for Windows stay out of host builds.
    if !on_windows {
        for package in packages.iter().filter(|p| p.windows_only) {
            workspace.extend(["--exclude".to_owned(), package.name.clone()]);
        }
    }
    let host = |words: &[&str]| Step::new(words).then_owned(&workspace);

    let mut host_steps = Vec::new();
    if full {
        host_steps.push(Step::new(&["fmt", "--all", "--check"]));
    }
    if code {
        host_steps.push(host(&["clippy", "--all-targets"]).then(&["--", "-D", "warnings"]));
        match &tests {
            Some(filter) if filter.is_empty() => {}
            Some(filter) => host_steps
                .push(host(&["nextest", "run", "--no-tests=pass"]).then(&["-E", filter.as_str()])),
            None => host_steps.push(host(&["nextest", "run", "--no-tests=pass"])),
        }
    }
    if docs {
        host_steps.push(host(&["test", "--doc"]));
    }
    if full {
        host_steps.push(host(&["doc", "--no-deps"]).env("RUSTDOCFLAGS", "-D warnings"));
        host_steps.push(Step::new(&["deny", "check"]));
    }
    // Off Windows the whole workspace is also linted for Windows, where the
    // `cfg(windows)` code builds.
    let windows = (!on_windows && code).then(|| {
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
