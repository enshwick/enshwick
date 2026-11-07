//! `cargo enshwick generate`: the typed API from a build's reflection. With
//! an executable it snapshots the build's covered types in the game data
//! folder, outside the repository; it then writes the API crate's `typed`
//! modules and game screens and the loader's id table from the newest kept
//! server build's snapshot, or with `--check` fails when they differ from
//! what is committed.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, anyhow, bail};
use enshwick_gamedata::reflect::{Image, Layout, Reflection};
use enshwick_gamedata::typed::{Catalogue, RENAMES, Renames, build_name, snapshot_in};
use enshwick_gamedata::{Side, Table, exact, file_name, newest, tables_in};

/// The one hand-written file among the generated modules.
const RUNTIME: &str = "crates/enshwick/src/typed/runtime.rs";
/// The folder of the generated modules.
const TYPED: &str = "crates/enshwick/src/typed";

/// The kept builds' tables in the game data folder `data`.
///
/// # Errors
///
/// When the folder or a table does not read.
pub(crate) fn tables_of(data: &Path) -> anyhow::Result<Vec<Table>> {
    tables_in(data)
        .with_context(|| data.display().to_string())?
        .iter()
        .map(|(file, text)| Table::parse(text).with_context(|| file.clone()))
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

/// The repository's rename table; empty when there is none yet.
///
/// # Errors
///
/// When the table does not read.
pub(crate) fn renames(root: &Path) -> anyhow::Result<Renames> {
    let path = root.join(RENAMES);
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

/// Writes `catalogue` as the snapshot of `side`'s build `fingerprint` in
/// the game data folder `data`.
///
/// # Errors
///
/// When the file does not write.
pub(crate) fn write_snapshot(
    data: &Path,
    catalogue: &Catalogue,
    side: Side,
    fingerprint: enshwick_gamedata::Fingerprint,
) -> anyhow::Result<PathBuf> {
    let path = snapshot_in(data, &file_name(side, fingerprint));
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).with_context(|| dir.display().to_string())?;
    }
    std::fs::write(&path, catalogue.snapshot()).with_context(|| path.display().to_string())?;
    Ok(path)
}

/// Snapshots `from` into the game data folder `data` when given, then
/// writes the typed API from the newest kept server build's snapshot, or
/// with `check` fails when the committed files differ from what it would
/// write.
///
/// # Errors
///
/// When `root` is not the enshwick repository, a file does not read or
/// write, the build has no table, the snapshot is missing, or with `check`,
/// when a generated file differs.
pub(crate) fn run(
    root: &Path,
    data: &Path,
    from: Option<&Path>,
    check: bool,
) -> anyhow::Result<String> {
    if !root.join(RUNTIME).is_file() {
        bail!(
            "{} is not the enshwick repository: {RUNTIME} is missing; pass --root",
            root.display()
        );
    }
    let tables = tables_of(data)?;
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
        let path = write_snapshot(data, &catalogue, side, fingerprint)?;
        done.push(format!("snapshot {}", path.display()));
    }
    let table = newest(&tables, Side::Server)
        .ok_or_else(|| anyhow!("{}: no server build table", data.display()))?;
    let path = snapshot_in(data, &table.file_name());
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

    /// A demo server build's table, kept in a game data folder.
    const TABLE: &str = "name = \"demo (1)\"\nside = \"server\"\nguid = \"00000000000000000000000000000001\"\nage = 2\nsteam_build = 3\n";

    /// A repository, with a game data folder in it whose one table is a
    /// demo server build's and whose snapshot is the gamedata fixture.
    fn repo(name: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("typed-generate-{name}-{}", std::process::id()));
        drop(std::fs::remove_dir_all(&root));
        let table = Table::parse(TABLE).unwrap();
        let tables = data(&root).join("tables");
        std::fs::create_dir_all(&tables).unwrap();
        std::fs::write(tables.join(table.file_name()), TABLE).unwrap();
        let snapshot = snapshot_in(&data(&root), &table.file_name());
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

    fn data(root: &Path) -> PathBuf {
        root.join("data")
    }

    #[test]
    fn the_typed_api_is_written_then_checked_and_a_hand_edit_or_stale_module_fails() {
        let root = repo("check");
        let data = data(&root);
        let run = |check| run(&root, &data, None, check);
        let wrote = run(false).unwrap();
        assert!(wrote.starts_with("wrote 4 files"), "{wrote}");
        assert!(wrote.ends_with("server build 2 (Steam 3)"), "{wrote}");
        assert!(run(true).unwrap().starts_with("the typed API matches"));
        assert_eq!(
            std::fs::read_to_string(root.join(RUNTIME)).unwrap(),
            "// hand-written\n"
        );

        let ecs = root.join(TYPED).join("ecs.rs");
        std::fs::write(&ecs, "// edited\n").unwrap();
        let error = run(true).unwrap_err().to_string();
        assert!(error.contains("ecs.rs"), "{error}");

        let stale = root.join(TYPED).join("gone.rs");
        std::fs::write(&stale, "").unwrap();
        run(false).unwrap();
        assert!(!stale.exists(), "a stale module is removed");
        std::fs::write(&stale, "").unwrap();
        let error = run(true).unwrap_err().to_string();
        assert!(error.contains("gone.rs (stale)"), "{error}");
        drop(std::fs::remove_dir_all(&root));
    }

    #[test]
    fn a_rename_table_reads_and_a_broken_one_is_an_error() {
        let root = repo("renames");
        assert_eq!(renames(&root).unwrap(), Renames::default());
        let table = root.join(RENAMES);
        std::fs::create_dir_all(table.parent().unwrap()).unwrap();
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
        let error = run(&root, &data(&root), None, false)
            .unwrap_err()
            .to_string();
        assert!(error.contains("not the enshwick repository"), "{error}");
        assert!(!root.join(TYPED).exists());
        drop(std::fs::remove_dir_all(&root));
    }

    #[test]
    fn a_data_folder_without_a_server_table_is_named_in_the_error() {
        let root = repo("tableless");
        let empty = root.join("gamedata-without-tables");
        std::fs::create_dir_all(&empty).unwrap();
        let error = run(&root, &empty, None, false).unwrap_err().to_string();
        assert!(
            error.contains("gamedata-without-tables") && error.contains("no server build table"),
            "{error}"
        );
        drop(std::fs::remove_dir_all(&root));
    }
}
