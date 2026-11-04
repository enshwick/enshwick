//! `cargo enshwick generate`: the typed API from a build's reflection. With
//! an executable it snapshots the build's covered types beside the build
//! tables; it then writes the API crate's `typed` modules and the loader's id
//! table from the newest server build's snapshot, or with `--check` fails
//! when they differ from what is committed.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, anyhow, bail};
use enshwick_gamedata::reflect::{Image, Layout, Reflection};
use enshwick_gamedata::typed::{Catalogue, Renames};
use enshwick_gamedata::{Side, Table, embedded, exact, file_name, newest};

/// Where the snapshots live, below the repository root.
pub(crate) const SNAPSHOTS: &str = "crates/enshwick-gamedata/typed";
/// The rename table, beside the snapshots.
pub(crate) const RENAMES: &str = "renames.toml";
/// The API crate's sources, below the repository root.
const API_SRC: &str = "crates/enshwick/src";
/// The loader's id table, below the repository root.
const IDS: &str = "crates/enshwick-gamedata/src/typed/ids.rs";
/// The one hand-written file among the generated modules.
const RUNTIME: &str = "typed/runtime.rs";

fn tables() -> anyhow::Result<Vec<Table>> {
    embedded()
        .iter()
        .map(|(file, text)| Table::parse(text).with_context(|| (*file).to_owned()))
        .collect()
}

/// The words a generated file's header names a build by.
fn build_name(table: &Table) -> String {
    let age = table.fingerprint().age();
    table.steam_build().map_or_else(
        || format!("{} build {age}", table.side().as_str()),
        |steam| format!("{} build {age} (Steam {steam})", table.side().as_str()),
    )
}

/// The snapshot file of the build `table` is for.
pub(crate) fn snapshot_path(root: &Path, table_file: &str) -> PathBuf {
    root.join(SNAPSHOTS)
        .join(Path::new(table_file).with_extension("jsonl"))
}

/// Snapshots the covered types of the executable `exe` and returns the
/// snapshot's path.
///
/// # Errors
///
/// When the executable does not read, holds no reflection data or has no
/// build table.
pub(crate) fn snapshot(root: &Path, exe: &Path) -> anyhow::Result<PathBuf> {
    let bytes = std::fs::read(exe).with_context(|| exe.display().to_string())?;
    let image =
        Image::parse(&bytes, Layout::File, None).with_context(|| exe.display().to_string())?;
    let fingerprint = image
        .fingerprint()
        .ok_or_else(|| anyhow!("{} names no build", exe.display()))?;
    let side = [Side::Server, Side::Client]
        .into_iter()
        .find(|side| exact(&tables().unwrap_or_default(), *side, fingerprint).is_some())
        .ok_or_else(|| anyhow!("{}: no build table; pin it first", exe.display()))?;
    let catalogue = Catalogue::from_reflection(&Reflection::scan(&image));
    if catalogue.snapshot().is_empty() {
        bail!("{} holds no reflection data", exe.display());
    }
    let path = snapshot_path(root, &file_name(side, fingerprint));
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).with_context(|| dir.display().to_string())?;
    }
    std::fs::write(&path, catalogue.snapshot()).with_context(|| path.display().to_string())?;
    Ok(path)
}

/// The rename table beside the snapshots; empty when there is none yet.
///
/// # Errors
///
/// When the table does not read.
pub(crate) fn renames(root: &Path) -> anyhow::Result<Renames> {
    let path = root.join(SNAPSHOTS).join(RENAMES);
    match std::fs::read_to_string(&path) {
        Ok(text) => Renames::parse(&text).with_context(|| path.display().to_string()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Renames::default()),
        Err(e) => Err(e).with_context(|| path.display().to_string()),
    }
}

/// Every generated file of the newest server build's snapshot, by path
/// below `root`.
fn generated(root: &Path) -> anyhow::Result<(BTreeMap<PathBuf, String>, String)> {
    let tables = tables()?;
    let table = newest(&tables, Side::Server).ok_or_else(|| anyhow!("no server build table"))?;
    let path = snapshot_path(root, &table.file_name());
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("{}: run with --from the build's executable", path.display()))?;
    let catalogue = Catalogue::from_snapshot(&text).with_context(|| path.display().to_string())?;
    let build = build_name(table);
    let mut files: BTreeMap<PathBuf, String> = catalogue
        .rust(&build)?
        .into_iter()
        .map(|(file, text)| (root.join(API_SRC).join(file), text))
        .collect();
    files.insert(root.join(IDS), catalogue.ids_file(&build, &renames(root)?)?);
    Ok((files, build))
}

/// The generated modules already in `root` that no longer belong there.
fn stale(root: &Path, files: &BTreeMap<PathBuf, String>) -> Vec<PathBuf> {
    let dir = root.join(API_SRC).join("typed");
    let runtime = root.join(API_SRC).join(RUNTIME);
    std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension().is_some_and(|e| e == "rs")
                && *path != runtime
                && !files.contains_key(path)
        })
        .collect()
}

/// Snapshots `from` when given, then writes the typed API, or with `check`
/// fails when the committed files differ from what it would write.
///
/// # Errors
///
/// When a file does not read or write, the snapshot is missing, or with
/// `check`, when a generated file differs.
pub(crate) fn run(root: &Path, from: Option<&Path>, check: bool) -> anyhow::Result<String> {
    let mut done = Vec::new();
    if let Some(exe) = from {
        let exe = crate::types::executable(exe)?;
        done.push(format!("snapshot {}", snapshot(root, &exe)?.display()));
    }
    let (files, build) = generated(root)?;
    let stale = stale(root, &files);
    if check {
        let mut differ: Vec<String> = files
            .iter()
            .filter(|(path, text)| std::fs::read_to_string(path).ok().as_ref() != Some(*text))
            .map(|(path, _)| path.display().to_string())
            .collect();
        differ.extend(stale.iter().map(|p| format!("{} (stale)", p.display())));
        if !differ.is_empty() {
            bail!(
                "the typed API differs from {build}'s snapshot; run cargo enshwick generate:\n{}",
                differ.join("\n")
            );
        }
        return Ok(format!("the typed API matches {build}"));
    }
    for (path, text) in &files {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).with_context(|| dir.display().to_string())?;
        }
        std::fs::write(path, text).with_context(|| path.display().to_string())?;
    }
    for path in &stale {
        std::fs::remove_file(path).with_context(|| path.display().to_string())?;
    }
    done.push(format!(
        "wrote {} files of the typed API from {build}",
        files.len()
    ));
    Ok(done.join("\n"))
}
