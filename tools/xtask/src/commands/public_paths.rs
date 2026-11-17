//! The public paths of the API crate, `tools/public-paths.txt`, in no package:
//! every module, item and member a mod can name after `enshwick::`, from
//! rustdoc's JSON of the crate and of the workspace crates it re-exports,
//! built once with its default features and once with `typed`, the widest,
//! which builds on `unstable`. The docs
//! site reads the file at the ref it pins, so the paths its coverage table
//! names are checked against what the compiler sees, and a rename shows in
//! the pull request that makes it.

mod walk;

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::Command;

use rustdoc_types::{Crate, FORMAT_VERSION};

use crate::{FACADE, cargo};

/// The committed list, relative to the workspace root.
pub(crate) const FILE: &str = "tools/public-paths.txt";
/// The mark of a path only an unstable build has.
const UNSTABLE: &str = "unstable";
/// The facade's widest feature: every unstable path, the generated typed API
/// among them, which the coverage table names too (`content::removal`).
const WIDEST: &str = "typed";
/// Its own target folder: `RUSTC_BOOTSTRAP` changes what cargo builds with,
/// so in the workspace's target it would rebuild everything on each switch.
const TARGET_DIR: &str = "target/public-paths";
/// rustdoc's JSON output is unstable; the pinned stable rustdoc writes it
/// when told it may, as the merged doctests run.
const RUSTDOC_FLAGS: &str = "-Z unstable-options --output-format json --document-private-items --document-hidden-items --cap-lints allow";
/// The file's opening lines.
const HEADER: &str = "\
# The public paths of the enshwick crate, one a line, each followed by
# ` unstable` when only the `unstable` feature, or `typed` on it, compiles it.
# Written by `cargo xtask public-paths`; `cargo xtask check` refuses a stale copy.
";
/// How many differing paths a stale list's problem names.
const SHOWN: usize = 10;

/// `cargo xtask public-paths`: rewrites the committed list.
pub(crate) fn run(root: &Path) -> Result<bool, String> {
    let list = generate(root)?;
    let path = root.join(FILE);
    std::fs::write(&path, list).map_err(|error| format!("{}: {error}", path.display()))?;
    Ok(true)
}

/// The problems with the committed list under `root`: the paths it lacks
/// or names that the crate no longer has, a few of each.
pub(crate) fn check(root: &Path) -> Vec<String> {
    let fresh = match generate(root) {
        Ok(list) => list,
        Err(error) => return vec![error],
    };
    let committed = std::fs::read_to_string(root.join(FILE)).unwrap_or_default();
    stale(&committed, &fresh)
}

/// What sets `committed` apart from `fresh`, a few lines of each side.
fn stale(committed: &str, fresh: &str) -> Vec<String> {
    let lines = |text: &str| -> BTreeSet<String> {
        text.lines()
            .filter(|line| !line.starts_with('#'))
            .map(str::to_owned)
            .collect()
    };
    let (committed, fresh) = (lines(committed), lines(fresh));
    if committed == fresh {
        return Vec::new();
    }
    let mut problems = vec![format!(
        "{FILE} is stale: run `cargo xtask public-paths` and commit it"
    )];
    for (sign, side, other) in [("+", &fresh, &committed), ("-", &committed, &fresh)] {
        let differing: Vec<&String> = side.difference(other).collect();
        problems.extend(
            differing
                .iter()
                .take(SHOWN)
                .map(|line| format!("  {sign} {line}")),
        );
        if differing.len() > SHOWN {
            problems.push(format!("  {sign} … {} more", differing.len() - SHOWN));
        }
    }
    problems
}

/// The list as the crate under `root` has it now.
fn generate(root: &Path) -> Result<String, String> {
    let documented = documented(root)?;
    let stable = paths(root, &documented, false)?;
    let unstable = paths(root, &documented, true)?;
    Ok(render(&stable, &unstable))
}

/// The list's text: every path either build has, sorted, those only the
/// `unstable` build has marked.
fn render(stable: &BTreeSet<String>, unstable: &BTreeSet<String>) -> String {
    let mut text = HEADER.to_owned();
    for path in stable.union(unstable) {
        text.push_str(path);
        if !stable.contains(path) {
            text.push(' ');
            text.push_str(UNSTABLE);
        }
        text.push('\n');
    }
    text
}

// The public paths of the facade, documented with its widest feature or/n/// with its default ones.
fn paths(root: &Path, documented: &[String], widest: bool) -> Result<BTreeSet<String>, String> {
    let mut command = Command::new(cargo());
    command
        .current_dir(root)
        .env("RUSTC_BOOTSTRAP", "1")
        .env("RUSTDOCFLAGS", RUSTDOC_FLAGS)
        .args(["doc", "--quiet", "--no-deps", "--target-dir", TARGET_DIR]);
    for name in documented {
        command.args(["--package", name]);
    }
    if widest {
        command.args(["--features", &format!("{FACADE}/{WIDEST}")]);
    }
    let output = command
        .output()
        .map_err(|error| format!("running cargo doc: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "cargo doc for the public paths failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let doc = root.join(TARGET_DIR).join("doc");
    let mut crates = BTreeMap::new();
    for name in documented {
        let lib = name.replace('-', "_");
        let file = doc.join(format!("{lib}.json"));
        let text = std::fs::read_to_string(&file)
            .map_err(|error| format!("{}: {error}", file.display()))?;
        let krate: Crate = serde_json::from_str(&text).map_err(|error| {
            format!(
                "{}: {error} (rustdoc-types reads format {FORMAT_VERSION}; move it with the toolchain)",
                file.display()
            )
        })?;
        crates.insert(lib, krate);
    }
    walk::Surface::new(&crates).paths(FACADE)
}

/// The facade and the workspace crates it builds on, its optional ones
/// left out: selecting only these keeps their features as a build of the
/// facade unifies them.
fn documented(root: &Path) -> Result<Vec<String>, String> {
    let output = Command::new(cargo())
        .current_dir(root)
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .output()
        .map_err(|error| format!("running cargo metadata: {error}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned());
    }
    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("reading cargo metadata: {error}"))?;
    Ok(built_on(&metadata, FACADE))
}

/// `package` and the workspace packages it depends on through normal,
/// required dependencies, directly or not, from `cargo metadata --no-deps`.
fn built_on(metadata: &serde_json::Value, package: &str) -> Vec<String> {
    let deps: BTreeMap<&str, Vec<&str>> = metadata["packages"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let deps = entry["dependencies"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|dep| {
                    dep["kind"].is_null() && dep["path"].is_string() && dep["optional"] != true
                })
                .filter_map(|dep| dep["name"].as_str())
                .collect();
            Some((entry["name"].as_str()?, deps))
        })
        .collect();
    let mut reached = BTreeSet::new();
    let mut pending = vec![package];
    while let Some(name) = pending.pop() {
        if reached.insert(name) {
            pending.extend(deps.get(name).into_iter().flatten());
        }
    }
    reached.into_iter().map(str::to_owned).collect()
}

#[cfg(test)]
mod tests;
