//! Repository tasks, run as `cargo xtask <task>`: one module per task in
//! [`commands`], and the rules `cargo xtask check` holds the tree to
//! without compiling in [`check`].
//!
//! `cargo xtask compat` reports what works on a game build and ports the
//! loader's entries to a new one. It links the loader, so it is the binary
//! `xtask-compat` of this package, built with the `compat` feature;
//! `cargo xtask compat` builds and runs it, and
//! `cargo run -p xtask --bin xtask-compat --features compat -- <exe>` runs it
//! directly.

#![forbid(unsafe_code)]

mod affected;
mod cargo_files;
mod check;
mod commands;
mod scratch;

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use affected::Package;
use clap::Parser as _;

/// The API crate, the one crate a mod names: the facade over the internal
/// crates, whose rendered docs hold every public item.
pub(crate) const FACADE: &str = "enshwick";

/// The tasks of the repository.
#[derive(Debug, clap::Parser)]
#[command(name = "cargo xtask", about = "The repository's own tasks")]
struct Cli {
    #[command(subcommand)]
    task: Task,
}

#[derive(Debug, clap::Subcommand)]
enum Task {
    /// Runs one criterion benchmark and compares it with main without
    /// building main again, or keeps its numbers on a clean main commit.
    Bench(commands::bench::Args),
    /// Runs the compiling checks a change needs before it merges.
    Check(commands::check::Args),
    /// Writes `enshwick.zip`, the loader players unpack into the game folder.
    Dist {
        /// The folder to write it into.
        #[arg(long, default_value = "target/dist")]
        out: PathBuf,
        /// Refuses to write the archive without `ENSHWICK_SIGNING_KEY`, as a
        /// release does.
        #[arg(long)]
        signed: bool,
    },
    /// Writes the notes of the alpha the workspace carries.
    Notes(commands::notes::Args),
    /// Closes every changelog for the alpha the workspace carries, or those
    /// of the named packages at their own versions, joining each crate's
    /// change files.
    Close(commands::close::Args),
    /// Moves the install and dependency lines of the Markdown to the
    /// workspace version's tag.
    TagLines,
    /// Rewrites the ABI's C header from the ABI module.
    Header,
    /// Checks the links rustdoc leaves unchecked in the API crate's pages.
    DocLinks,
    /// Rewrites the API crate's list of public paths from rustdoc's JSON.
    PublicPaths,
    /// Peels a module's `Load` methods into forwarders, `<from>=<into>` each.
    PeelLoad {
        /// The modules, each `<from>=<into>`.
        #[arg(required = true)]
        pairs: Vec<String>,
    },
    /// Prints the cargo arguments of a host build, one a line, for CI.
    HostArgs,
    /// What works on a game build, and porting the loader's entries to a
    /// new one; runs `xtask-compat` with the arguments as they are.
    #[command(disable_help_flag = true)]
    Compat {
        /// The arguments of `xtask-compat`; `--help` lists them.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

fn main() -> ExitCode {
    run(Cli::parse().task).unwrap_or_else(|error| {
        let _shown = writeln!(std::io::stderr(), "xtask: {error}");
        ExitCode::FAILURE
    })
}

/// Runs `task`; `compat` passes on the exit code of `xtask-compat`, 2
/// included.
fn run(task: Task) -> Result<ExitCode, String> {
    let root = workspace_root();
    let passed = match task {
        Task::Bench(args) => commands::bench::run(&args)?,
        Task::Check(args) => commands::check::run(&args)?,
        Task::Dist { out, signed } => commands::dist::run(&out, signed)?,
        Task::Notes(args) => commands::notes::run(&args)?,
        Task::Close(args) => {
            say(&commands::close::run(&root, &packages()?, &args)?);
            true
        }
        Task::TagLines => commands::tag_lines::run()?,
        Task::Header => commands::header::run(&root)?,
        Task::DocLinks => commands::doc_links::run(&root)?,
        Task::PublicPaths => commands::public_paths::run(&root)?,
        Task::PeelLoad { pairs } => commands::peel::run(&root, &pairs)?,
        Task::HostArgs => {
            // One argument a line, for CI runs that test the host as the check does.
            for arg in commands::check::host_args(&packages()?, cfg!(windows)) {
                let _shown = writeln!(std::io::stdout(), "{arg}");
            }
            true
        }
        Task::Compat { args } => return Ok(compat_task(&args)),
    };
    Ok(if passed {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

/// The workspace root, two levels above this crate.
fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
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
    let libs: BTreeMap<&str, Option<String>> = list
        .iter()
        .filter_map(|p| Some((p["name"].as_str()?, lib_name(p))))
        .collect();
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
            let (deps, doc_deps) = split_deps(package, &libs);
            let lib = lib_name(package);
            Package {
                name: package["name"].as_str().unwrap_or_default().to_owned(),
                version: package["version"].as_str().unwrap_or_default().to_owned(),
                dir,
                deps,
                doc_deps,
                lib,
                windows_only: package["metadata"]["xtask"]["windows-only"].as_bool() == Some(true),
                windows_tests: ["windows-tests", "windows-only"]
                    .iter()
                    .any(|key| package["metadata"]["xtask"][key].as_bool() == Some(true)),
                compiles_c: package["metadata"]["xtask"]["compiles-c"].as_bool() == Some(true),
                stable_build: package["features"].get("unstable").is_some()
                    || stable_examples(package),
                features: package["features"]
                    .as_object()
                    .into_iter()
                    .flat_map(|features| features.keys())
                    .filter(|feature| *feature != "default")
                    .cloned()
                    .collect(),
                feature_table: package["features"]
                    .as_object()
                    .into_iter()
                    .flatten()
                    .map(|(feature, list)| {
                        let on = list.as_array().into_iter().flatten();
                        let on = on.filter_map(|entry| entry.as_str().map(str::to_owned));
                        (feature.clone(), on.collect())
                    })
                    .collect(),
            }
        })
        .collect())
}

/// The name Rust paths give the library target of `package`, a `cargo
/// metadata` entry, if it has one.
fn lib_name(package: &serde_json::Value) -> Option<String> {
    package["targets"]
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
        .map(|name| name.replace('-', "_"))
}

/// The target kind of a build script, which has no tests and cannot use a
/// dev-dependency.
const BUILD_SCRIPT: &str = "custom-build";

/// The name the library of `package`, a `cargo metadata` entry, gives
/// itself ([`affected::idents::self_alias`]), when the library is its only
/// target with tests: an integration test, bench, example or binary is a
/// crate of its own, where the alias does not hold.
fn self_alias(package: &serde_json::Value) -> Option<String> {
    let targets = package["targets"].as_array()?;
    let mut lib = None;
    for target in targets {
        let kinds = target["kind"].as_array()?;
        if kinds.iter().any(|kind| kind == BUILD_SCRIPT) {
            continue;
        }
        if !kinds
            .iter()
            .all(|kind| LIB_KINDS.contains(&kind.as_str().unwrap_or_default()))
        {
            return None;
        }
        lib = target["src_path"].as_str();
    }
    affected::idents::self_alias(&std::fs::read_to_string(lib?).ok()?)
}

/// The workspace packages `package`, a `cargo metadata` entry, depends on:
/// those its builds or tests use, and the dev-dependencies only its doc
/// examples can use ([`Package::doc_deps`]): those its library shadows by
/// the name it gives itself ([`self_alias`]). `libs` gives each workspace
/// package's library name.
fn split_deps(
    package: &serde_json::Value,
    libs: &BTreeMap<&str, Option<String>>,
) -> (Vec<String>, Vec<String>) {
    let mut built = BTreeSet::new();
    // Each dev-dependency with a library, by the name its code writes it by.
    let mut dev = BTreeMap::new();
    for dep in package["dependencies"].as_array().into_iter().flatten() {
        let Some((name, lib)) = dep["name"]
            .as_str()
            .and_then(|name| libs.get_key_value(name))
        else {
            continue;
        };
        let written = dep["rename"]
            .as_str()
            .map(|rename| rename.replace('-', "_"))
            .or_else(|| lib.clone());
        match written {
            Some(written) if dep["kind"] == "dev" => {
                dev.insert(written, (*name).to_owned());
            }
            _ => {
                built.insert((*name).to_owned());
            }
        }
    }
    let alias = if dev.is_empty() {
        None
    } else {
        self_alias(package)
    };
    let mut doc_deps = Vec::new();
    for (written, name) in dev {
        if alias.as_ref() != Some(&written) || built.contains(&name) {
            built.insert(name);
        } else {
            doc_deps.push(name);
        }
    }
    (built.into_iter().collect(), doc_deps)
}

/// Whether the examples of `package`, a `cargo metadata` entry, name the
/// facade as a stable mod does: through a dev-dependency on it that turns on
/// no feature beyond `test`.
fn stable_examples(package: &serde_json::Value) -> bool {
    package["dependencies"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|dep| {
            dep["name"] == FACADE
                && dep["kind"] == "dev"
                && dep["features"]
                    .as_array()
                    .is_some_and(|features| features.iter().all(|feature| feature == "test"))
        })
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

pub(crate) fn cargo() -> String {
    std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned())
}

/// `PATH` for the tools the checks run, with LLVM's default Windows folder
/// last: its installer leaves the folder off `PATH` unless asked, and the
/// header check and the C example need its clang.
pub(crate) fn tool_path() -> std::ffi::OsString {
    let path = std::env::var_os("PATH").unwrap_or_default();
    let llvm = std::env::var_os("ProgramFiles")
        .filter(|_| cfg!(windows))
        .map(|programs| Path::new(&programs).join("LLVM").join("bin"));
    std::env::join_paths(std::env::split_paths(&path).chain(llvm)).unwrap_or(path)
}

fn say(text: &str) {
    let _shown = writeln!(std::io::stderr(), "xtask: {text}");
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn the_domain_crates_use_the_facade_only_in_their_doc_examples() {
        let packages = packages().unwrap();
        let ui = packages.iter().find(|p| p.name == "enshwick-ui").unwrap();
        assert!(ui.doc_deps.iter().any(|dep| dep == FACADE), "{ui:?}");
        assert!(!ui.deps.iter().any(|dep| dep == FACADE), "{ui:?}");
    }

    #[test]
    fn a_self_alias_holds_only_when_the_library_is_the_only_crate_with_tests() {
        let dir = scratch::Scratch::new("alias").unwrap();
        let lib = dir.join("lib.rs");
        std::fs::write(&lib, "extern crate self as enshwick;").unwrap();
        let target = |kind: &str| serde_json::json!({ "kind": [kind], "src_path": lib });
        let package = |targets: &[serde_json::Value]| serde_json::json!({ "targets": targets });
        let alone = self_alias(&package(&[target("lib"), target(BUILD_SCRIPT)]));
        let with_test = self_alias(&package(&[target("lib"), target("test")]));
        assert_eq!(alone.as_deref(), Some(FACADE));
        assert_eq!(with_test, None);
    }

    #[test]
    fn the_command_line_reads_each_task_and_passes_compat_s_arguments_on() {
        let parse = |args: &[&str]| Cli::try_parse_from(["cargo xtask"].iter().chain(args));
        let check = parse(&["check", "--part", "host", "--part", "windows-tests"]).unwrap();
        assert!(matches!(check.task, Task::Check(_)));
        assert!(parse(&["check", "--part", "doc"]).is_err());
        let Task::Compat { args } = parse(&["compat", "pin", "--old", "a", "--help"])
            .unwrap()
            .task
        else {
            panic!("not compat");
        };
        assert_eq!(args, ["pin", "--old", "a", "--help"]);
        assert!(parse(&["peel-load"]).is_err());
        assert!(parse(&["close"]).is_err());
        assert!(parse(&["close", "--date", "2026-11-13"]).is_ok());
        assert!(
            parse(&[
                "close",
                "--date",
                "2026-11-13",
                "--package",
                "a",
                "--package",
                "b"
            ])
            .is_ok()
        );
    }
}
