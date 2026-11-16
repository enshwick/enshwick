//! A wide local change runs the tests of the packages it only reaches one
//! process per test binary, not one per test as nextest does: under the
//! load of several batches a test process takes 0.3 to 1.3 s to start, and
//! the loader's 1,902 tests took 2,000 to 5,500 test-seconds under nextest
//! but 12 s in one process (2026-11-16). A package whose code names the
//! fake host stays on nextest, one process per test, as the fake host needs; so
//! do the changed packages and every CI run. A test that fails here runs
//! again under nextest in a process of its own, so one that needs no other
//! test beside it passes, and a real failure fails twice.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use super::Step;
use crate::affected::{Package, Scope, idents};
use crate::{cargo, say, tool_path, workspace_root};

/// The fake host's type: a package whose code names it keeps one
/// process per test.
const FAKE_HOST: &str = "FakeHost";

/// The longest a test binary runs before it is stopped and its package
/// rerun under nextest: tests sharing one process can wait on each other
/// forever, and the longest test took 39 s (2026-11-16).
const BINARY_LIMIT: Duration = Duration::from_secs(300);

/// How often a running test binary is polled for its exit.
const POLL: Duration = Duration::from_millis(50);

/// The line libtest ends a run with, which a binary that stopped early
/// never prints.
const SUMMARY: &str = "test result: ";

/// The packages a wide `scope` only tests whose tests run one process per
/// binary when the host part runs (`host`): those that test on this host
/// (`host_ok`) and whose code does not name the fake host.
pub(super) fn packages(
    scope: &Scope,
    host: bool,
    packages: &[Package],
    host_ok: impl Fn(&String) -> bool,
) -> BTreeSet<String> {
    let Scope::Some {
        tested, wide: true, ..
    } = scope
    else {
        return BTreeSet::new();
    };
    if !host {
        return BTreeSet::new();
    }
    let root = workspace_root();
    packages
        .iter()
        .filter(|package| tested.contains(&package.name) && host_ok(&package.name))
        .filter(|package| !idents::names(&root.join(&package.dir), FAKE_HOST))
        .map(|package| package.name.clone())
        .collect()
}

/// One test binary of a package.
#[derive(Debug, PartialEq, Eq)]
struct Binary {
    package: String,
    version: String,
    executable: PathBuf,
    /// The package's directory, where cargo runs its tests.
    dir: PathBuf,
}

impl Binary {
    /// Runs the binary's tests as cargo test would, within [`BINARY_LIMIT`]:
    /// its standard output when it ended, `None` when it did not start or
    /// was stopped.
    fn run(&self) -> Option<(bool, String)> {
        // Cargo puts the folders of the build's own libraries first on the
        // search path, where Windows looks for a test's DLLs.
        let deps = self.executable.parent()?;
        let path = std::env::join_paths(
            [deps.to_path_buf(), deps.parent()?.to_path_buf()]
                .into_iter()
                .chain(std::env::split_paths(&tool_path())),
        )
        .ok()?;
        let mut command = Command::new(&self.executable);
        command
            .current_dir(&self.dir)
            .env("CARGO_MANIFEST_DIR", &self.dir)
            .env("CARGO_PKG_NAME", &self.package)
            .env("CARGO_PKG_VERSION", &self.version)
            .env("PATH", path)
            .arg("--quiet");
        finish(command, BINARY_LIMIT)
    }
}

/// Runs `command` for at most `limit`: whether it passed and what it wrote
/// to its standard output with its summary ([`SUMMARY`]), or `None` when it
/// did not start or was stopped.
fn finish(mut command: Command, limit: Duration) -> Option<(bool, String)> {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    let reader = thread::spawn(move || {
        let mut text = String::new();
        stdout.read_to_string(&mut text).map(|_| text)
    });
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().ok()? {
            break status;
        }
        if started.elapsed() > limit {
            say(&format!(
                "stopped {} after {} s",
                command.get_program().to_string_lossy(),
                limit.as_secs()
            ));
            // Either fails only for a child that already ended.
            child.kill().ok();
            child.wait().ok();
            return None;
        }
        thread::sleep(POLL);
    };
    let stdout = reader.join().ok()?.ok()?;
    Some((status.success() && stdout.contains(SUMMARY), stdout))
}

/// Runs the tests of `names`, built with the cargo arguments `workspace`,
/// one process per binary, then each failed test again under nextest;
/// whether they all passed.
pub(super) fn run(names: &BTreeSet<String>, workspace: &[String]) -> bool {
    let started = Instant::now();
    let Some(binaries) = binaries(workspace, names) else {
        return false;
    };
    if binaries.is_empty() {
        say("FAILED: cargo test --no-run built no test binary of these packages");
        return false;
    }
    // Failed tests by package; `None`: rerun the whole package.
    let mut again: BTreeMap<String, Option<BTreeSet<String>>> = BTreeMap::new();
    for binary in &binaries {
        let failed = match binary.run() {
            Some((true, _)) => continue,
            Some((false, stdout)) => failures(&stdout),
            None => None,
        };
        let entry = again
            .entry(binary.package.clone())
            .or_insert_with(|| Some(BTreeSet::new()));
        match (entry.as_mut(), failed) {
            (Some(all), Some(failed)) => all.extend(failed),
            _ => *entry = None,
        }
    }
    let seconds = started.elapsed().as_secs_f32();
    let packages: Vec<&str> = names.iter().map(String::as_str).collect();
    say(&format!(
        "{} in {seconds:.1} s: the tests of {} in one process per binary",
        if again.is_empty() {
            "ok"
        } else {
            "some failed"
        },
        packages.join(", ")
    ));
    if again.is_empty() {
        return true;
    }
    say(&format!(
        "running {} again under nextest, one process per test",
        retry_filter(&again)
    ));
    Step::new(&["nextest", "run", "--no-tests=fail"])
        .then_owned(workspace)
        .then(&["-E", &retry_filter(&again)])
        .run(false)
}

/// The test binaries of `names` among those `cargo test --no-run` builds
/// with the arguments `workspace`: the ones nextest built a moment before.
/// `None`, with cargo's errors said, when the build fails.
fn binaries(workspace: &[String], names: &BTreeSet<String>) -> Option<Vec<Binary>> {
    let output = Command::new(cargo())
        .args([
            "test",
            "--no-run",
            "--message-format",
            "json-diagnostic-rendered-ansi",
        ])
        .args(workspace)
        .env("PATH", tool_path())
        .output();
    let output = match output {
        Ok(output) => output,
        Err(error) => {
            say(&format!(
                "FAILED: cargo test --no-run did not start: {error}"
            ));
            return None;
        }
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    if output.status.success() {
        return Some(artifacts(&stdout, names));
    }
    say("FAILED: cargo test --no-run");
    for rendered in stdout
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter_map(|message| message["message"]["rendered"].as_str().map(str::to_owned))
    {
        say(&rendered);
    }
    say(&String::from_utf8_lossy(&output.stderr));
    None
}

/// The test binaries of `names` in cargo's JSON messages `text`.
fn artifacts(text: &str, names: &BTreeSet<String>) -> Vec<Binary> {
    text.lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|message| message["reason"] == "compiler-artifact")
        .filter(|message| message["profile"]["test"] == true)
        .filter_map(|message| {
            let executable = message["executable"].as_str()?;
            let manifest = Path::new(message["manifest_path"].as_str()?);
            // A package id ends `#<name>@<version>`, or `#<version>` when the
            // name is the directory's.
            let dir = manifest.parent()?;
            let (name, version) = package_of(message["package_id"].as_str()?, dir)?;
            let package = names.get(name)?;
            Some(Binary {
                package: package.clone(),
                version: version.to_owned(),
                executable: executable.into(),
                dir: dir.to_path_buf(),
            })
        })
        .collect()
}

/// The package name and version a cargo package id names, for a package
/// in `dir`.
fn package_of<'a>(id: &'a str, dir: &'a Path) -> Option<(&'a str, &'a str)> {
    let fragment = id.rsplit_once('#')?.1;
    match fragment.split_once('@') {
        Some(named) => Some(named),
        None => Some((dir.file_name()?.to_str()?, fragment)),
    }
}

/// The names of the failed tests libtest's `stdout` lists last; `None`
/// when it lists none, as when the binary stopped before its summary.
fn failures(stdout: &str) -> Option<BTreeSet<String>> {
    let (_, list) = stdout.rsplit_once("\nfailures:\n")?;
    let failed: BTreeSet<String> = list
        .lines()
        .map_while(|line| line.strip_prefix("    "))
        .map(str::to_owned)
        .collect();
    (!failed.is_empty()).then_some(failed)
}

/// The nextest filter of the tests `again` names, a whole package where it
/// names no test.
fn retry_filter(again: &BTreeMap<String, Option<BTreeSet<String>>>) -> String {
    again
        .iter()
        .flat_map(|(package, tests)| match tests {
            None => vec![format!("package({package})")],
            Some(tests) => tests
                .iter()
                .map(|test| format!("(package({package}) & test(={test}))"))
                .collect(),
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_failures_are_the_names_libtest_lists_last() {
        let stdout = "running 3 tests\n..F\nfailures:\n\n---- a::b stdout ----\npanicked\n\n\
                      failures:\n    a::b\n    c::d\n\ntest result: FAILED. 1 passed; 2 failed\n";
        assert_eq!(
            failures(stdout),
            Some(["a::b".to_owned(), "c::d".to_owned()].into())
        );
    }

    #[test]
    fn a_binary_that_stopped_before_its_summary_lists_no_failures() {
        assert_eq!(
            failures("running 3 tests\n..\nerror: process didn't exit\n"),
            None
        );
    }

    #[test]
    fn a_failed_test_reruns_alone_and_a_binary_that_stopped_its_whole_package() {
        let again = BTreeMap::from([
            ("loader".to_owned(), Some(["w::a".to_owned()].into())),
            ("values".to_owned(), None),
        ]);
        assert_eq!(
            retry_filter(&again),
            "(package(loader) & test(=w::a)) | package(values)"
        );
    }

    #[test]
    fn the_test_binaries_of_the_named_packages_come_from_cargo_s_messages() {
        let text = [
            r#"{"reason":"compiler-artifact","package_id":"path+file:///D:/w/crates/runtime/loader#enshwick-loader@1.0.0","manifest_path":"D:/w/crates/runtime/loader/Cargo.toml","profile":{"test":true},"executable":"D:/w/target/loader.exe"}"#,
            r#"{"reason":"compiler-artifact","package_id":"path+file:///D:/w/crates/runtime/loader#enshwick-loader@1.0.0","manifest_path":"D:/w/crates/runtime/loader/Cargo.toml","profile":{"test":false},"executable":null}"#,
            r#"{"reason":"compiler-artifact","package_id":"path+file:///D:/w/crates/enshwick#1.0.0","manifest_path":"D:/w/crates/enshwick/Cargo.toml","profile":{"test":true},"executable":"D:/w/target/main.exe"}"#,
            "not json",
        ]
        .join("\n");
        let names = ["enshwick-loader".to_owned()].into();
        assert_eq!(
            artifacts(&text, &names),
            [Binary {
                package: "enshwick-loader".to_owned(),
                version: "1.0.0".to_owned(),
                executable: "D:/w/target/loader.exe".into(),
                dir: "D:/w/crates/runtime/loader".into(),
            }]
        );
        let facade = ["enshwick".to_owned()].into();
        let binaries = artifacts(&text, &facade);
        assert_eq!(
            binaries
                .iter()
                .map(|b| b.version.as_str())
                .collect::<Vec<_>>(),
            ["1.0.0"]
        );
    }

    /// This test binary, running only `test`.
    fn this_binary_running(test: &str, ignored: bool) -> Command {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command.args([
            "--exact",
            &format!("commands::check::in_process::tests::{test}"),
        ]);
        if ignored {
            command.arg("--ignored");
        }
        command
    }

    #[test]
    #[ignore = "the child the time limit stops"]
    fn a_child_that_sleeps_past_the_limit() {
        thread::sleep(Duration::from_secs(60));
    }

    #[test]
    fn a_binary_that_runs_past_its_limit_is_stopped() {
        let started = Instant::now();
        let stopped = finish(
            this_binary_running("a_child_that_sleeps_past_the_limit", true),
            Duration::from_secs(2),
        );
        assert_eq!(stopped, None);
        assert!(started.elapsed() < Duration::from_secs(30));
    }

    #[test]
    fn a_binary_passes_only_with_its_summary() {
        let passed = finish(
            this_binary_running("the_failures_are_the_names_libtest_lists_last", false),
            BINARY_LIMIT,
        );
        assert!(passed.is_some_and(|(ok, stdout)| ok && stdout.contains(SUMMARY)));
        let mut silent = Command::new(cargo());
        silent.arg("--version");
        assert!(finish(silent, BINARY_LIMIT).is_some_and(|(ok, _)| !ok));
        let missing = Command::new(std::env::temp_dir().join("xtask-no-such-test.exe"));
        assert_eq!(finish(missing, BINARY_LIMIT), None);
    }
}
