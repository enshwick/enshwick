//! `cargo xtask bench <name>`: one criterion benchmark, compared with main
//! without building main again. A run on a clean checkout of a main commit
//! keeps its bench executable under the commit's id; a run anywhere else
//! runs the newest one kept for its base on main and then its own, back to
//! back, so both see the same load. The executables live in the clone's own
//! `target/kept-benches`, which every worktree of the clone shares, unless
//! [`KEPT_VAR`] names another folder. A kept executable runs from the
//! branch's package folder, with the branch's files, and under the build
//! settings it was kept with; the commit pins the toolchain, but not
//! settings such as `RUSTFLAGS` set for one run.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::SystemTime;

use crate::commands::check::git;
use crate::{cargo, say};

/// The branch whose executables are kept.
const MAIN: &str = "origin/main";
/// How many main commits before the base a kept executable may be.
const LOOKBACK: usize = 50;
/// The folder of the kept executables, in the clone's target.
const KEPT: &str = "kept-benches";
/// The variable naming another folder for them, such as one that several
/// clones share.
const KEPT_VAR: &str = "ENSH_KEPT_BENCHES";

#[derive(Debug, clap::Args)]
pub(crate) struct Args {
    /// The bench target, as its `[[bench]]` entry names it.
    name: String,
}

/// What a run does beside running the branch's benchmark.
#[derive(Debug, PartialEq, Eq)]
enum Plan {
    /// Keeps the executable for the main commit.
    Keep(String),
    /// Runs the executable kept for the main commit first, `behind` commits
    /// before the base, and compares with its numbers.
    Compare { commit: String, behind: usize },
    /// Runs alone: no main commit near the base has a kept executable.
    Alone,
}

/// The bench target of a package.
struct Target {
    package: String,
    dir: PathBuf,
    features: Vec<String>,
}

pub(crate) fn run(args: &Args) -> Result<bool, String> {
    let target = bench_target(&args.name)?;
    let kept_dir = kept_dir()?;
    let kept = |commit: &str| {
        kept_dir
            .join(commit)
            .join(&args.name)
            .with_extension(std::env::consts::EXE_EXTENSION)
    };
    if git(&["fetch", "--quiet", "origin", "main"]).is_none() {
        say(&format!(
            "could not fetch main: comparing with {MAIN} as it is here"
        ));
    }
    let base = line(&["merge-base", "HEAD", MAIN]).ok_or("no merge base with origin/main")?;
    let on_main = line(&["rev-parse", "HEAD"]).as_ref() == Some(&base)
        && git(&["status", "--porcelain"]).is_some_and(|status| status.trim().is_empty());
    let history = git(&[
        "rev-list",
        "--first-parent",
        &format!("--max-count={LOOKBACK}"),
        &base,
    ])
    .ok_or_else(|| format!("git rev-list could not list main before {base}"))?;
    let history: Vec<&str> = history.lines().collect();
    let plan = plan(on_main, &base, &history, |commit| kept(commit).is_file());
    let built = build(&target, &args.name)?;
    let bench = |exe: &Path, criterion: &[&str]| {
        Command::new(exe)
            .arg("--bench")
            .args(criterion)
            .current_dir(&target.dir)
            .status()
            .map_err(|error| format!("running {}: {error}", exe.display()))
            .map(|status| status.success())
    };
    match plan {
        Plan::Keep(commit) => {
            let to = kept(&commit);
            say(&format!("keeping main at {commit} in {}", to.display()));
            keep(&built, &to)?;
            prune(&kept_dir);
            bench(&built, &[])
        }
        Plan::Compare { commit, behind } => {
            say(&format!(
                "running main at {commit}, {behind} commits before the base, then the branch"
            ));
            let baseline = format!("main-{commit}");
            let [main, branch] = compare_flags(&baseline);
            Ok(bench(&kept(&commit), &main)? && bench(&built, &branch)?)
        }
        Plan::Alone => {
            say(&format!(
                "no main build kept within {LOOKBACK} commits of {base}: run \
                 `cargo xtask bench {}` on a clean checkout of {MAIN} to keep one",
                args.name
            ));
            bench(&built, &[])
        }
    }
}

/// What to do: keep the executable on a clean main commit, else compare
/// with the newest commit of `history`, the base first, that `saved` holds.
fn plan(on_main: bool, base: &str, history: &[&str], saved: impl Fn(&str) -> bool) -> Plan {
    if on_main {
        return Plan::Keep(base.to_owned());
    }
    history
        .iter()
        .position(|commit| saved(commit))
        .map_or(Plan::Alone, |behind| Plan::Compare {
            commit: history[behind].to_owned(),
            behind,
        })
}

/// The criterion flags of main's run and the branch's: lenient, so a
/// benchmark the branch adds runs alone instead of failing the comparison.
fn compare_flags(baseline: &str) -> [[&str; 2]; 2] {
    [
        ["--save-baseline", baseline],
        ["--baseline-lenient", baseline],
    ]
}

/// Copies `from` to `to` under a name of its own first, so a run in another
/// worktree never starts a half-written executable; one kept already stays.
fn keep(from: &Path, to: &Path) -> Result<(), String> {
    if to.is_file() {
        return Ok(());
    }
    let dir = to.parent().ok_or("a kept executable has no folder")?;
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let partial = to.with_extension(format!("partial-{}", std::process::id()));
    std::fs::copy(from, &partial).map_err(|e| format!("{}: {e}", partial.display()))?;
    if let Err(error) = std::fs::rename(&partial, to) {
        let _removed = std::fs::remove_file(&partial);
        if !to.is_file() {
            return Err(format!("{}: {error}", to.display()));
        }
    }
    Ok(())
}

/// Removes the kept commits beyond the newest [`LOOKBACK`], which no run
/// compares with any more; a folder in use stays until a later run.
fn prune(kept_dir: &Path) {
    let Ok(entries) = std::fs::read_dir(kept_dir) else {
        return;
    };
    let dirs = entries.filter_map(Result::ok).filter_map(|entry| {
        let changed = entry.metadata().ok()?.modified().ok()?;
        Some((entry.path(), changed))
    });
    for dir in stale(dirs.collect()) {
        let _removed = std::fs::remove_dir_all(dir);
    }
}

/// The kept folders past the newest [`LOOKBACK`] by when they changed.
fn stale(mut dirs: Vec<(PathBuf, SystemTime)>) -> Vec<PathBuf> {
    dirs.sort_by_key(|dir| std::cmp::Reverse(dir.1));
    dirs.into_iter()
        .skip(LOOKBACK)
        .map(|(dir, _)| dir)
        .collect()
}

/// Builds the bench target `name` as `cargo bench` does and gives its
/// executable.
fn build(target: &Target, name: &str) -> Result<PathBuf, String> {
    let output = Command::new(cargo())
        .args(["bench", "--no-run", "--message-format=json"])
        .args(["--package", &target.package, "--bench", name])
        .args(if target.features.is_empty() {
            Vec::new()
        } else {
            vec!["--features".to_owned(), target.features.join(",")]
        })
        .stderr(Stdio::inherit())
        .output()
        .map_err(|error| format!("running cargo bench: {error}"))?;
    if !output.status.success() {
        return Err(format!("cargo bench could not build {name}"));
    }
    executable(&String::from_utf8_lossy(&output.stdout), name)
        .ok_or_else(|| format!("cargo bench named no executable for {name}"))
}

/// The executable cargo's JSON `messages` give the bench target `name`.
fn executable(messages: &str, name: &str) -> Option<PathBuf> {
    messages
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .find(|message| {
            message["reason"] == "compiler-artifact"
                && message["target"]["name"] == name
                && message["target"]["kind"] == serde_json::json!(["bench"])
        })
        .and_then(|message| message["executable"].as_str().map(PathBuf::from))
}

/// The kept executables' folder: [`KEPT_VAR`], else the one in the clone
/// that holds this worktree's git folder.
fn kept_dir() -> Result<PathBuf, String> {
    if let Some(dir) = std::env::var_os(KEPT_VAR).filter(|dir| !dir.is_empty()) {
        return Ok(PathBuf::from(dir));
    }
    let common = line(&["rev-parse", "--path-format=absolute", "--git-common-dir"])
        .ok_or("not in a git checkout")?;
    let clone = Path::new(&common)
        .parent()
        .ok_or("the git folder has no parent")?;
    Ok(clone.join("target").join(KEPT))
}

/// The package, folder and required features of the bench target `name`.
fn bench_target(name: &str) -> Result<Target, String> {
    let output = Command::new(cargo())
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .output()
        .map_err(|error| format!("running cargo metadata: {error}"))?;
    if !output.status.success() {
        return Err("cargo metadata failed".to_owned());
    }
    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("reading cargo metadata: {error}"))?;
    for package in metadata["packages"].as_array().into_iter().flatten() {
        for target in package["targets"].as_array().into_iter().flatten() {
            if target["name"] == name && target["kind"] == serde_json::json!(["bench"]) {
                let features = target["required-features"].as_array().into_iter().flatten();
                let missing = |field| format!("cargo metadata gave {name} no package {field}");
                let manifest = Path::new(
                    package["manifest_path"]
                        .as_str()
                        .ok_or_else(|| missing("manifest_path"))?,
                );
                return Ok(Target {
                    package: package["name"]
                        .as_str()
                        .ok_or_else(|| missing("name"))?
                        .to_owned(),
                    dir: manifest
                        .parent()
                        .ok_or_else(|| missing("folder"))?
                        .to_owned(),
                    features: features
                        .filter_map(|f| f.as_str().map(str::to_owned))
                        .collect(),
                });
            }
        }
    }
    Err(format!("no bench target {name}"))
}

/// The first line git prints for `args`.
fn line(args: &[&str]) -> Option<String> {
    git(args)?.lines().next().map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    const HISTORY: [&str; 3] = ["c3", "c2", "c1"];

    #[test]
    fn a_clean_main_commit_keeps_its_executable_under_its_own_id() {
        assert_eq!(
            plan(true, "c3", &HISTORY, |_| true),
            Plan::Keep("c3".to_owned())
        );
    }

    #[test]
    fn a_branch_compares_with_the_newest_kept_main_commit() {
        let plan = plan(false, "c3", &HISTORY, |commit| commit != "c3");
        assert_eq!(
            plan,
            Plan::Compare {
                commit: "c2".to_owned(),
                behind: 1
            }
        );
    }

    #[test]
    fn a_branch_runs_alone_when_no_main_commit_is_kept() {
        assert_eq!(plan(false, "c3", &HISTORY, |_| false), Plan::Alone);
    }

    #[test]
    fn the_branch_compares_leniently_so_a_new_benchmark_runs_alone() {
        assert_eq!(
            compare_flags("main-c2"),
            [
                ["--save-baseline", "main-c2"],
                ["--baseline-lenient", "main-c2"]
            ]
        );
    }

    #[test]
    fn keeping_writes_the_executable_whole_and_leaves_one_kept_already() {
        let dir = std::env::temp_dir().join(format!("xtask-bench-keep-{}", std::process::id()));
        let from = dir.join("built.exe");
        let to = dir.join("c3").join("client_build.exe");
        std::fs::create_dir_all(&dir).expect("temp folder");
        std::fs::write(&from, b"new").expect("built executable");
        keep(&from, &to).expect("first keep");
        std::fs::write(&from, b"newer").expect("rebuilt executable");
        keep(&from, &to).expect("second keep");
        let kept = std::fs::read(&to).expect("kept executable");
        let leftovers = std::fs::read_dir(to.parent().expect("folder"))
            .expect("kept folder")
            .count();
        std::fs::remove_dir_all(&dir).expect("cleanup");
        assert_eq!((kept.as_slice(), leftovers), (b"new".as_slice(), 1));
    }

    #[test]
    fn pruning_drops_only_the_kept_commits_past_the_newest_lookback() {
        let start = SystemTime::UNIX_EPOCH;
        let dirs: Vec<_> = (0..LOOKBACK as u64 + 2)
            .map(|age| {
                let changed = start + std::time::Duration::from_secs(1_000 - age);
                (PathBuf::from(format!("c{age}")), changed)
            })
            .collect();
        let expected: Vec<_> = (LOOKBACK as u64..LOOKBACK as u64 + 2)
            .map(|age| PathBuf::from(format!("c{age}")))
            .collect();
        assert_eq!(stale(dirs), expected);
    }

    #[test]
    fn the_executable_is_the_bench_artifact_of_the_named_target() {
        let messages = [
            r#"{"reason":"compiler-artifact","target":{"name":"client_build","kind":["lib"]},"executable":null}"#,
            r#"{"reason":"compiler-artifact","target":{"name":"other","kind":["bench"]},"executable":"o.exe"}"#,
            r#"{"reason":"compiler-artifact","target":{"name":"client_build","kind":["bench"]},"executable":"c.exe"}"#,
            r#"{"reason":"build-finished","success":true}"#,
        ]
        .join("\n");
        assert_eq!(
            executable(&messages, "client_build"),
            Some(PathBuf::from("c.exe"))
        );
        assert_eq!(executable(&messages, "missing"), None);
    }
}
