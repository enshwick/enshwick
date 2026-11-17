//! The public paths of the API crate, `tools/public-paths.txt`, in no package:
//! every module, item and member a mod can name after `enshwick::`, from
//! rustdoc's JSON of the crate and of the workspace crates its build
//! compiles, built once with its default features and once with `typed`,
//! which builds on `unstable`; `test` and `schema`, which serve tools and
//! tests, are left out. The docs
//! site reads the file at the ref it pins, so the paths its coverage table
//! names are checked against what the compiler sees, and a rename shows in
//! the pull request that makes it.

mod walk;

use std::collections::{BTreeMap, BTreeSet};
use std::io::ErrorKind;
use std::path::Path;
use std::process::Command;

use rustdoc_types::{Crate, FORMAT_VERSION};

use crate::affected::Package;
use crate::commands::check::Step;
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
# The public paths of the enshwick crate with its default features or `typed`,
# one a line, each followed by ` unstable` when only the `unstable` feature, or
# `typed` on it, compiles it.
# Written by `cargo xtask public-paths`; `cargo xtask check` refuses a stale copy.
";
/// How many differing paths a stale list's problem names.
const SHOWN: usize = 10;

/// `cargo xtask public-paths`: rewrites the committed list.
pub(crate) fn run(root: &Path) -> Result<bool, String> {
    let list = generate(root, &crate::packages()?)?;
    let path = root.join(FILE);
    std::fs::write(&path, list).map_err(|error| format!("{}: {error}", path.display()))?;
    Ok(true)
}

/// The problems with the committed list under `root`: the paths it lacks
/// or names that the crate no longer has, a few of each.
pub(crate) fn check(root: &Path, packages: &[Package]) -> Vec<String> {
    let fresh = match generate(root, packages) {
        Ok(list) => list,
        Err(error) => return vec![error],
    };
    let path = root.join(FILE);
    let committed = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == ErrorKind::NotFound => String::new(),
        Err(error) => return vec![format!("{}: {error}", path.display())],
    };
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
fn generate(root: &Path, packages: &[Package]) -> Result<String, String> {
    let stable = paths(root, packages, None)?;
    let unstable = paths(root, packages, Some(&format!("{FACADE}/{WIDEST}")))?;
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

/// The public paths of the facade, documented with `features` or with its
/// default ones.
fn paths(
    root: &Path,
    packages: &[Package],
    features: Option<&str>,
) -> Result<BTreeSet<String>, String> {
    let documented = documented(root, packages, features)?;
    let target = root.join(TARGET_DIR);
    let mut step = Step::new(&[
        "doc",
        "--no-deps",
        "--target-dir",
        &target.display().to_string(),
    ])
    .env("RUSTC_BOOTSTRAP", "1")
    .env("RUSTDOCFLAGS", RUSTDOC_FLAGS);
    for (name, _) in &documented {
        step = step.then(&["--package", name]);
    }
    if let Some(features) = features {
        step = step.then(&["--features", features]);
    }
    if !step.run(false) {
        return Err("cargo doc for the public paths failed".to_owned());
    }
    let doc = target.join("doc");
    let mut crates = BTreeMap::new();
    for (_, lib) in documented {
        let file = doc.join(format!("{lib}.json"));
        let text = std::fs::read_to_string(&file)
            .map_err(|error| format!("{}: {error}", file.display()))?;
        let krate: Crate = serde_json::from_str(&text).map_err(|error| {
            format!(
                "{}: {error} (rustdoc-types reads format {FORMAT_VERSION}; move it with the toolchain)",
                file.display()
            )
        })?;
        if krate.format_version != FORMAT_VERSION {
            return Err(format!(
                "{}: rustdoc wrote format {}, rustdoc-types reads {FORMAT_VERSION}; move it with the toolchain",
                file.display(),
                krate.format_version
            ));
        }
        crates.insert(lib, krate);
    }
    walk::Surface::new(&crates).paths(FACADE)
}

/// The workspace packages, name and library, a build of the facade with
/// `features` compiles, optional ones its features turn on included:
/// selecting only these keeps their features as that build unifies them.
fn documented(
    root: &Path,
    packages: &[Package],
    features: Option<&str>,
) -> Result<Vec<(String, String)>, String> {
    let mut command = Command::new(cargo());
    command.current_dir(root).args([
        "tree",
        "--package",
        FACADE,
        "--edges",
        "normal",
        "--prefix",
        "none",
        "--format",
        "{p}",
    ]);
    if let Some(features) = features {
        command.args(["--features", features]);
    }
    let output = command
        .output()
        .map_err(|error| format!("running cargo tree: {error}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned());
    }
    let libraries = packages
        .iter()
        .filter_map(|package| Some((package.name.as_str(), package.lib.as_deref()?)))
        .collect();
    Ok(in_tree(
        &String::from_utf8_lossy(&output.stdout),
        &libraries,
    ))
}

/// The workspace packages among the lines of `tree`, `cargo tree --prefix
/// none --format {p}` output, each with its library from `libraries`.
fn in_tree(tree: &str, libraries: &BTreeMap<&str, &str>) -> Vec<(String, String)> {
    let named: BTreeSet<&str> = tree
        .lines()
        .filter_map(|line| line.split_whitespace().next())
        .collect();
    libraries
        .iter()
        .filter(|(name, _)| named.contains(*name))
        .map(|(name, lib)| ((*name).to_owned(), (*lib).to_owned()))
        .collect()
}

#[cfg(test)]
mod tests;
