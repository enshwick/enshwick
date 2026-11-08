//! A build's compatibility report data: what the loader finds on it, as
//! the `reports/` files hold it.

use std::fmt::Write as _;
use std::io::Write as _;
use std::path::Path;

use enshwick_gamedata as tables;

use super::{Args, Build, MODULE_IDS, build, build_name};

/// The report data of `build` as TOML: names, counts and ids only. An entry
/// is missing when a route or an anchor of the side names it and this build
/// does not have it, and unread when it is an attribute slot and no game
/// archive beside the executable reads; each loader feature that names a
/// missing entry is listed with them.
pub(super) fn data(build: &Build) -> Result<String, String> {
    if MODULE_IDS.is_empty() {
        return Err("only a Windows build of this tool knows the loader's features".to_owned());
    }
    let table = &build.table;
    let mut known: std::collections::BTreeSet<&str> =
        build.route_ids.iter().map(String::as_str).collect();
    known.extend(build.anchors.symbols().iter().map(tables::Symbol::id));
    known.extend(build.anchors.constants().iter().map(tables::Constant::id));
    // Without the game's archive the slots are unknown, not missing: the
    // game always has its archive, a kept executable may not. An archive
    // that is there but does not give them leaves them missing.
    let unread: Vec<&str> = match &build.slots {
        Err(_) if !build.archive => table
            .attributes()
            .iter()
            .map(|(id, _)| id.as_str())
            .collect(),
        _ => Vec::new(),
    };
    // An `<id>.bytes` check is not an entry, and a route drops it.
    known.retain(|id| id.rsplit('.').next() != Some("bytes") && !unread.contains(id));
    let missing: Vec<&str> = known
        .iter()
        .copied()
        .filter(|&id| table.symbol(id).is_none() && table.constant(id).is_none())
        .collect();
    let list = |ids: &[&str]| {
        let quoted: Vec<String> = ids.iter().map(|id| format!("{id:?}")).collect();
        format!("[{}]", quoted.join(", "))
    };
    let mut out = String::from(
        "# What the loader finds on this game build; `cargo xtask compat <exe> --data <file>`\n# writes it.\n\n",
    );
    let _w = writeln!(out, "build = {:?}", build.name);
    let _w = writeln!(out, "side = {:?}", build.side.as_str());
    if let Some(steam) = build.steam {
        let _w = writeln!(out, "steam_build = {steam}");
    }
    if let Some(version) = build.game_version {
        let _w = writeln!(out, "game_version = {version}");
    }
    let _w = writeln!(
        out,
        "entries = {}",
        table.symbols().len() + table.constants().len()
    );
    let _w = writeln!(out, "routed = {}", build.routed);
    let _w = writeln!(out, "missing = {}", list(&missing));
    if !unread.is_empty() {
        let _w = writeln!(out, "unread = {}", list(&unread));
    }
    for &(name, ids) in MODULE_IDS {
        let named = ids.iter().filter(|id| known.contains(**id)).count();
        let lacks: Vec<&str> = ids
            .iter()
            .copied()
            .filter(|id| missing.contains(id))
            .collect();
        let Some(state) = feature_state(lacks.len(), named) else {
            continue;
        };
        let _w = writeln!(
            out,
            "\n[[feature]]\nname = {name:?}\nstate = {state:?}\nmissing = {}",
            list(&lacks)
        );
    }
    Ok(out)
}

/// The state a report gives a loader feature that names `named` entries of
/// the build and lacks `lacking` of them; `None` when it lacks none.
fn feature_state(lacking: usize, named: usize) -> Option<&'static str> {
    match lacking {
        0 => None,
        n if n == named => Some("off"),
        _ => Some("partly"),
    }
}

/// The `refresh` step: rewrites the report in `dir` of each executable the
/// `ENSH_EXES` path list (`exes`) names, only once every one of them reads,
/// so a failed run leaves no report half refreshed.
pub(super) fn refresh(dir: &Path, exes: Option<std::ffi::OsString>) -> Result<(), String> {
    let list = exes
        .filter(|list| !list.is_empty())
        .ok_or("ENSH_EXES names no game executable")?;
    let reports = std::env::split_paths(&list)
        .map(|exe| report_data(dir, &exe).map(|report| (exe, report)))
        .collect::<Result<Vec<_>, _>>()?;
    for (exe, (file, text)) in reports {
        let path = dir.join(&file);
        std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
        let _shown = writeln!(std::io::stderr(), "{file}: {}", exe.display());
    }
    Ok(())
}

/// The report data of `exe` and the name of its file in `dir`; where no
/// file beside the executable states the Steam build or the game version,
/// the report already there does.
pub(super) fn report_data(dir: &Path, exe: &Path) -> Result<(String, String), String> {
    let mut build = build(&Args {
        exe: exe.to_path_buf(),
        mods: Vec::new(),
        report: None,
        patterns: false,
        data: None,
        steam_build: None,
        game_version: None,
    })?;
    let file = tables::file_name(build.side, build.fingerprint);
    if let Ok(text) = std::fs::read_to_string(dir.join(&file)) {
        let stated: toml::Table = text.parse().map_err(|e| format!("{file}: {e}"))?;
        let number = |key: &str| {
            stated
                .get(key)
                .and_then(toml::Value::as_integer)
                .and_then(|n| u64::try_from(n).ok())
        };
        build.steam = build.steam.or_else(|| number("steam_build"));
        build.game_version = build.game_version.or_else(|| number("game_version"));
        build.name = build_name(
            build.side,
            build.steam,
            build.game_version,
            build.fingerprint,
        );
    }
    Ok((file, data(&build)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_feature_lacking_all_it_names_is_off_and_some_partly() {
        assert_eq!(feature_state(0, 3), None);
        assert_eq!(feature_state(3, 3), Some("off"));
        assert_eq!(feature_state(1, 3), Some("partly"));
    }

    /// A folder of its own for this test.
    fn empty_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("xtask-{name}-{}", std::process::id()));
        if dir.exists() {
            std::fs::remove_dir_all(&dir).unwrap();
        }
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn refresh_without_a_readable_executable_fails() {
        let dir = empty_dir("refresh-none");
        assert!(refresh(&dir, None).is_err());
        assert!(refresh(&dir, Some("".into())).is_err());
        let missing = dir.join("no-such-dir").join("enshrouded.exe");
        assert!(refresh(&dir, Some(missing.into_os_string())).is_err());
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
    }

    /// With `ENSH_SERVER_EXE`: one unreadable executable in the
    /// list leaves every report as it was.
    #[cfg(windows)]
    #[test]
    fn refresh_writes_no_report_when_one_executable_fails() {
        let Some(server) = std::env::var_os("ENSH_SERVER_EXE") else {
            return;
        };
        let dir = empty_dir("refresh-partial");
        let list =
            std::env::join_paths([std::path::PathBuf::from(server), dir.join("missing.exe")])
                .unwrap();
        assert!(refresh(&dir, Some(list)).is_err());
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
        let one = std::env::var_os("ENSH_SERVER_EXE");
        refresh(&dir, one).unwrap();
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
