//! The game data folder outside the repository: the kept builds' tables and
//! the typed snapshots `pin` reads and, with `--write`, keeps.

use std::path::{Path, PathBuf};

use anyhow::Context as _;
use enshwick_gamedata::reflect::Fingerprint;

use super::rows::Side;
use super::{Args, gamedata_side, spell, table};

/// The game data folder `--write` keeps the new build's snapshot in: `None`
/// without `--write` or outside the enshwick repository (the mods' holds no
/// typed API), checked before anything is written.
///
/// # Errors
///
/// With `--write` in the enshwick repository and no game data folder.
pub(super) fn snapshot_dir(args: &Args) -> anyhow::Result<Option<&Path>> {
    if !args.write || !args.repo.join(enshwick_gamedata::typed::RENAMES).is_file() {
        return Ok(None);
    }
    args.data.as_deref().map(Some).with_context(|| {
        format!(
            "no game data folder for the new build's snapshot: pass --data or set {}",
            enshwick_gamedata::DATA_VAR
        )
    })
}

/// The side of the old build `fingerprint`: the side of the table rows that
/// ported, else of a kept table in the game data folder `data`.
pub(super) fn old_side(
    data: Option<&Path>,
    ported: Option<Side>,
    fingerprint: Fingerprint,
) -> anyhow::Result<enshwick_gamedata::Side> {
    if let Some(side) = ported {
        return Ok(gamedata_side(side));
    }
    let tables = match data {
        Some(data) => crate::generate::tables_of(data)?,
        None => Vec::new(),
    };
    crate::generate::side_of(&tables, fingerprint)
        .with_context(|| format!("no table is for the old build {}", spell(fingerprint)))
}

/// The kept builds' tables in the game data folder `data`, sorted, each as
/// its path below the folder (`tables/<file>`), so a report never names
/// the private folder, and its text; none without it.
pub(super) fn table_files(data: Option<&Path>) -> anyhow::Result<Vec<(PathBuf, String)>> {
    let Some(data) = data else {
        return Ok(Vec::new());
    };
    let tables = enshwick_gamedata::tables_in(data).with_context(|| data.display().to_string())?;
    Ok(tables
        .into_iter()
        .map(|(file, text)| (Path::new(table::DIR).join(file), text))
        .collect())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn the_old_build_s_side_comes_from_the_repository_row_this_binary_may_not_know() {
        let unknown = Fingerprint::new([0xcd; 16], 1);
        assert_eq!(
            old_side(None, Some(Side::Client), unknown).unwrap(),
            enshwick_gamedata::Side::Client
        );
        assert!(old_side(None, None, unknown).is_err());
    }

    #[test]
    fn the_old_build_s_side_comes_from_a_kept_table_in_the_data_folder_given() {
        let data = std::env::temp_dir().join(format!("pin-old-side-{}", std::process::id()));
        fs::create_dir_all(data.join(table::DIR)).unwrap();
        fs::write(
            data.join(table::DIR).join("server-00000000000000000000000000000001-2.toml"),
            "name = \"demo\"\nside = \"server\"\nguid = \"00000000000000000000000000000001\"\nage = 2\n",
        )
        .unwrap();
        let mut guid = [0; 16];
        guid[15] = 1;
        let side = old_side(Some(&data), None, Fingerprint::new(guid, 2));
        let files = table_files(Some(&data));
        drop(fs::remove_dir_all(&data));
        assert_eq!(side.unwrap(), enshwick_gamedata::Side::Server);
        let files = files.unwrap();
        assert_eq!(files.len(), 1);
        assert!(files[0].0.is_relative(), "{:?}", files[0].0);
    }

    /// `--write` in the enshwick repository without a game data folder is
    /// refused before any file is written; outside it, or without
    /// `--write`, no folder is needed.
    #[test]
    fn a_write_without_a_data_folder_is_refused_before_anything_is_written() {
        let enshwick = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let args = |repo: PathBuf, write, data: Option<PathBuf>| Args {
            old: None,
            new: None,
            patterns: None,
            repo,
            data,
            steam_build: None,
            game_version: None,
            write,
            report: None,
            from: Vec::new(),
            from_file: None,
        };
        let refused = args(enshwick.clone(), true, None);
        let error = snapshot_dir(&refused).unwrap_err().to_string();
        assert!(error.contains(enshwick_gamedata::DATA_VAR), "{error}");
        let given = args(enshwick.clone(), true, Some("data".into()));
        assert_eq!(snapshot_dir(&given).unwrap(), Some(Path::new("data")));
        assert_eq!(snapshot_dir(&args(enshwick, false, None)).unwrap(), None);
        let mods = args(std::env::temp_dir(), true, None);
        assert_eq!(snapshot_dir(&mods).unwrap(), None);
    }
}
