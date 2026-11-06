//! `cargo enshwick generate`: the typed API from a build's reflection. With
//! an executable it snapshots the build's covered types beside the build
//! tables; it then writes the API crate's `typed` modules and game screens
//! and the loader's id table from the newest server build's snapshot, or with
//! `--check` fails
//! when they differ from what is committed.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, anyhow, bail};
use enshwick_gamedata::reflect::{Image, Layout, Reflection};
use enshwick_gamedata::typed::{Catalogue, RENAMES, Renames, SNAPSHOTS, build_name, snapshot_path};
use enshwick_gamedata::{Side, Table, embedded, exact, file_name, newest};

/// The one hand-written file among the generated modules.
const RUNTIME: &str = "crates/enshwick/src/typed/runtime.rs";
/// The folder of the generated modules.
const TYPED: &str = "crates/enshwick/src/typed";

/// The build tables compiled into this binary.
///
/// # Errors
///
/// When one does not read.
pub(crate) fn tables() -> anyhow::Result<Vec<Table>> {
    embedded()
        .iter()
        .map(|(file, text)| Table::parse(text).with_context(|| (*file).to_owned()))
        .collect()
}

/// The side whose table among `tables` is for the build `fingerprint`.
pub(crate) fn side_of(
    tables: &[Table],
    fingerprint: enshwick_gamedata::Fingerprint,
) -> Option<Side> {
    [Side::Server, Side::Client]
        .into_iter()
        .find(|side| exact(tables, *side, fingerprint).is_some())
}

/// The catalogue of the game executable `bytes` hold.
///
/// # Errors
///
/// When they do not read or hold no reflection data.
pub(crate) fn catalogue(bytes: &[u8]) -> anyhow::Result<Catalogue> {
    let image = Image::parse(bytes, Layout::File, None)?;
    let catalogue = Catalogue::from_reflection(&Reflection::scan(&image));
    if catalogue.snapshot().is_empty() {
        bail!("no reflection data");
    }
    Ok(catalogue)
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

/// Every generated file of `catalogue`, by path below `root`, with the
/// generated modules already there that no longer belong.
fn generated(
    root: &Path,
    catalogue: &Catalogue,
    build: &str,
) -> anyhow::Result<(BTreeMap<PathBuf, String>, Vec<PathBuf>)> {
    let files: BTreeMap<PathBuf, String> = catalogue
        .files(build, &renames(root)?)?
        .into_iter()
        .map(|(path, text)| (root.join(path), text))
        .collect();
    let runtime = root.join(RUNTIME);
    let stale = std::fs::read_dir(root.join(TYPED))
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension().is_some_and(|e| e == "rs")
                && *path != runtime
                && !files.contains_key(path)
        })
        .collect();
    Ok((files, stale))
}

/// Writes the typed API of `catalogue`, read from `build`, into `root`.
///
/// # Errors
///
/// When a file does not write.
pub(crate) fn write_api(root: &Path, catalogue: &Catalogue, build: &str) -> anyhow::Result<usize> {
    let (files, stale) = generated(root, catalogue, build)?;
    for (path, text) in &files {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).with_context(|| dir.display().to_string())?;
        }
        std::fs::write(path, text).with_context(|| path.display().to_string())?;
    }
    for path in &stale {
        std::fs::remove_file(path).with_context(|| path.display().to_string())?;
    }
    Ok(files.len())
}

/// Writes `catalogue` as the snapshot of `side`'s build `fingerprint`.
///
/// # Errors
///
/// When the file does not write.
pub(crate) fn write_snapshot(
    root: &Path,
    catalogue: &Catalogue,
    side: Side,
    fingerprint: enshwick_gamedata::Fingerprint,
) -> anyhow::Result<PathBuf> {
    let path = root.join(snapshot_path(&file_name(side, fingerprint)));
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).with_context(|| dir.display().to_string())?;
    }
    std::fs::write(&path, catalogue.snapshot()).with_context(|| path.display().to_string())?;
    Ok(path)
}

/// Snapshots `from` when given, then writes the typed API from the newest
/// server build's snapshot, or with `check` fails when the committed files
/// differ from what it would write.
///
/// # Errors
///
/// When `root` is not the enshwick repository, a file does not read or
/// write, the build has no table, the snapshot is missing, or with `check`,
/// when a generated file differs.
pub(crate) fn run(root: &Path, from: Option<&Path>, check: bool) -> anyhow::Result<String> {
    if !root.join(RUNTIME).is_file() {
        bail!(
            "{} is not the enshwick repository: {RUNTIME} is missing; pass --root",
            root.display()
        );
    }
    let tables = tables()?;
    let mut done = Vec::new();
    if let Some(exe) = from {
        let exe = crate::types::executable(exe)?;
        let bytes = std::fs::read(&exe).with_context(|| exe.display().to_string())?;
        let fingerprint = Image::parse(&bytes, Layout::File, None)?
            .fingerprint()
            .ok_or_else(|| anyhow!("{} names no build", exe.display()))?;
        let side = side_of(&tables, fingerprint)
            .ok_or_else(|| anyhow!("{}: no build table; pin it first", exe.display()))?;
        let catalogue = catalogue(&bytes).with_context(|| exe.display().to_string())?;
        let path = write_snapshot(root, &catalogue, side, fingerprint)?;
        done.push(format!("snapshot {}", path.display()));
    }
    let table = newest(&tables, Side::Server).ok_or_else(|| anyhow!("no server build table"))?;
    let path = root.join(snapshot_path(&table.file_name()));
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("{}: run with --from the build's executable", path.display()))?;
    let catalogue = Catalogue::from_snapshot(&text).with_context(|| path.display().to_string())?;
    let build = build_name(table);
    if check {
        let (files, stale) = generated(root, &catalogue, &build)?;
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
    let written = write_api(root, &catalogue, &build)?;
    done.push(format!(
        "wrote {written} files of the typed API from {build}"
    ));
    Ok(done.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A repository with the gamedata fixture as the newest server build's
    /// snapshot.
    fn repo(name: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("typed-generate-{name}-{}", std::process::id()));
        drop(std::fs::remove_dir_all(&root));
        let tables = tables().unwrap();
        let table = newest(&tables, Side::Server).unwrap();
        let snapshot = root.join(snapshot_path(&table.file_name()));
        std::fs::create_dir_all(snapshot.parent().unwrap()).unwrap();
        std::fs::write(
            &snapshot,
            include_str!("../../enshwick-gamedata/src/typed/fixture.jsonl"),
        )
        .unwrap();
        std::fs::create_dir_all(root.join(TYPED)).unwrap();
        std::fs::write(root.join(RUNTIME), "// hand-written\n").unwrap();
        root
    }

    #[test]
    fn the_typed_api_is_written_then_checked_and_a_hand_edit_or_stale_module_fails() {
        let root = repo("check");
        let wrote = run(&root, None, false).unwrap();
        assert!(wrote.starts_with("wrote 4 files"), "{wrote}");
        assert!(
            run(&root, None, true)
                .unwrap()
                .starts_with("the typed API matches")
        );
        assert_eq!(
            std::fs::read_to_string(root.join(RUNTIME)).unwrap(),
            "// hand-written\n"
        );

        let ecs = root.join(TYPED).join("ecs.rs");
        std::fs::write(&ecs, "// edited\n").unwrap();
        let error = run(&root, None, true).unwrap_err().to_string();
        assert!(error.contains("ecs.rs"), "{error}");

        let stale = root.join(TYPED).join("gone.rs");
        std::fs::write(&stale, "").unwrap();
        run(&root, None, false).unwrap();
        assert!(!stale.exists(), "a stale module is removed");
        std::fs::write(&stale, "").unwrap();
        let error = run(&root, None, true).unwrap_err().to_string();
        assert!(error.contains("gone.rs (stale)"), "{error}");
        drop(std::fs::remove_dir_all(&root));
    }

    #[test]
    fn a_rename_table_reads_and_a_broken_one_is_an_error() {
        let root = repo("renames");
        assert_eq!(renames(&root).unwrap(), Renames::default());
        let table = root.join(SNAPSHOTS).join(RENAMES);
        std::fs::write(&table, "[[types]]\nfrom = \"demo::A\"\nto = \"demo::B\"\n").unwrap();
        assert_ne!(renames(&root).unwrap(), Renames::default());
        std::fs::write(&table, "[[types]]\nfrom = 1\n").unwrap();
        assert!(renames(&root).is_err());
        drop(std::fs::remove_dir_all(&root));
    }

    #[test]
    fn a_folder_without_the_typed_runtime_is_refused_and_left_alone() {
        let root = repo("foreign");
        std::fs::remove_dir_all(root.join(TYPED)).unwrap();
        let error = run(&root, None, false).unwrap_err().to_string();
        assert!(error.contains("not the enshwick repository"), "{error}");
        assert!(!root.join(TYPED).exists());
        drop(std::fs::remove_dir_all(&root));
    }
}
