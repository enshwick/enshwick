//! A build's compatibility report data: what the loader finds on it, as
//! the `reports/` files hold it.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::io::Write as _;
use std::path::Path;

use enshwick_gamedata as tables;

use super::{Args, Build, MODULE_IDS, build, build_name, reports};

/// The report data of `build` as TOML, sealed for `inputs`: names, counts
/// and ids only. An entry is missing when a route or an anchor of the side
/// names it and this build does not have it, and unread when it is an
/// attribute slot and no game archive beside the executable reads; each
/// loader feature that names a missing entry is listed with them.
pub(super) fn data(build: &Build, inputs: &reports::Inputs) -> Result<String, String> {
    if MODULE_IDS.is_empty() {
        return Err("only a Windows build of this tool knows the loader's features".to_owned());
    }
    let table = &build.table;
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
    let known = known(
        build.route_ids.iter().map(String::as_str),
        &build.anchors,
        &unread,
    );
    let missing: Vec<&str> = known
        .iter()
        .copied()
        .filter(|&id| table.symbol(id).is_none() && table.constant(id).is_none())
        .collect();
    let mut out = String::from(
        "# What the loader finds on this game build; `cargo xtask compat refresh` writes it.\n# `sealed` ties it to the code that finds it, so a report edited by hand or\n# left behind by that code fails `cargo xtask check`.\n\n",
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
    out.push_str(&features(&known, &missing));
    Ok(reports::sealed(inputs, &out))
}

/// The entries a build's report counts: those the routes of its side look
/// for (`route_ids`) and its `anchors` name, but no `<id>.bytes` check, which
/// is not an entry, and no `unread` slot.
pub(super) fn known<'a>(
    route_ids: impl Iterator<Item = &'a str>,
    anchors: &'a tables::Table,
    unread: &[&str],
) -> BTreeSet<&'a str> {
    let mut known: BTreeSet<&str> = route_ids.collect();
    known.extend(anchors.symbols().iter().map(tables::Symbol::id));
    known.extend(anchors.constants().iter().map(tables::Constant::id));
    known.retain(|id| id.rsplit('.').next() != Some("bytes") && !unread.contains(id));
    known
}

/// The report's `[[feature]]` tables: each loader feature that lacks a
/// `missing` entry of those it names among the `known` ones.
pub(super) fn features(known: &BTreeSet<&str>, missing: &[&str]) -> String {
    let mut out = String::new();
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
    out
}

/// `ids` as a TOML array of strings.
fn list(ids: &[&str]) -> String {
    let quoted: Vec<String> = ids.iter().map(|id| format!("{id:?}")).collect();
    format!("[{}]", quoted.join(", "))
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

/// The `refresh` step: rewrites the report in the repository at `root` of
/// each executable the `ENSH_EXES` path list (`exes`) names, only once every
/// one of them reads and no other report would stay stale, so a failed run
/// leaves no report half refreshed.
pub(super) fn refresh(root: &Path, exes: Option<std::ffi::OsString>) -> Result<(), String> {
    let list = exes
        .filter(|list| !list.is_empty())
        .ok_or("ENSH_EXES names no game executable")?;
    let inputs = sealing_inputs(root)?;
    let dir = root.join(reports::REPORTS);
    let found = std::env::split_paths(&list)
        .map(|exe| report_data(&dir, &exe, &inputs).map(|report| (exe, report)))
        .collect::<Result<Vec<_>, _>>()?;
    let written: Vec<(String, String)> = found.iter().map(|(_, report)| report.clone()).collect();
    write(root, &written)?;
    for (exe, (file, _)) in &found {
        let _shown = writeln!(std::io::stderr(), "{file}: {}", exe.display());
    }
    Ok(())
}

/// The [`reports::inputs`] to write new seals with.
///
/// # Errors
///
/// As [`reports::inputs`], and while an untracked file sits among the
/// inputs: the new seals would leave it out, and once it is committed every
/// report would be stale.
pub(super) fn sealing_inputs(root: &Path) -> Result<reports::Inputs, String> {
    let untracked = reports::listed(root, "--others")?;
    if !untracked.is_empty() {
        let files: Vec<&str> = untracked.iter().map(String::as_str).collect();
        return Err(format!(
            "add these files to git or move them away before sealing reports: {}",
            files.join(", ")
        ));
    }
    reports::inputs(root)
}

/// Writes `found`, each a file name and its sealed text, into the reports
/// of the repository at `root`, only when no other report would stay stale.
///
/// # Errors
///
/// The reports that would stay stale, with nothing written, or a report
/// that does not write.
fn write(root: &Path, found: &[(String, String)]) -> Result<(), String> {
    let names: Vec<String> = found.iter().map(|(file, _)| file.clone()).collect();
    let left = reports::stale(root, &names);
    if !left.is_empty() {
        return Err(format!(
            "no report written: these would stay stale, so name their executables in ENSH_EXES too:\n{}",
            left.join("\n")
        ));
    }
    let dir = root.join(reports::REPORTS);
    for (file, text) in found {
        let path = dir.join(file);
        std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(())
}

/// The report data of `exe` and the name of its file in `dir`; where no
/// file beside the executable states the Steam build or the game version,
/// the report already there does.
pub(super) fn report_data(
    dir: &Path,
    exe: &Path,
    inputs: &reports::Inputs,
) -> Result<(String, String), String> {
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
    Ok((file, data(&build, inputs)?))
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

    #[test]
    fn a_report_counts_neither_byte_checks_nor_unread_slots() {
        let anchors = tables::anchors(tables::Side::Server).unwrap();
        let known = known(
            ["x.route", "x.route.bytes", "x.slot"].into_iter(),
            &anchors,
            &["x.slot"],
        );
        assert!(known.contains("x.route"));
        assert!(!known.contains("x.route.bytes"));
        assert!(!known.contains("x.slot"));
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
    fn an_untracked_file_among_the_inputs_refuses_new_seals() {
        let root = reports::tests::repository("unstaged");
        std::fs::write(
            root.join("crates/enshwick-gamedata/src/routes/walk.rs"),
            "fn walk() {}\n",
        )
        .unwrap();
        let refused = sealing_inputs(&root).unwrap_err();
        assert!(refused.contains("routes/walk.rs"), "{refused}");
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn writing_some_reports_refuses_while_another_would_stay_stale() {
        use reports::tests::{REPORT, ROUTE, repository, write_file};
        let root = repository("partial");
        let other = REPORT.replace("ab/1", "cd/1");
        let report = reports::sealed(&reports::inputs(&root).unwrap(), &other);
        write_file(
            &root,
            &format!("{}/server-cd-1.toml", reports::REPORTS),
            &report,
        );
        write_file(&root, ROUTE, "fn find() { todo!() }\n");
        let inputs = reports::inputs(&root).unwrap();
        let first = (
            "server-ab-1.toml".to_owned(),
            reports::sealed(&inputs, REPORT),
        );
        let refused = write(&root, std::slice::from_ref(&first)).unwrap_err();
        assert!(refused.contains("server-cd-1.toml"), "{refused}");
        assert_eq!(reports::stale(&root, &[]).len(), 2, "nothing was written");
        let second = (
            "server-cd-1.toml".to_owned(),
            reports::sealed(&inputs, &other),
        );
        write(&root, &[first, second]).unwrap();
        assert_eq!(reports::stale(&root, &[]), Vec::<String>::new());
        std::fs::remove_dir_all(&root).unwrap();
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
        let root = Path::new(crate::ROOT);
        let texts = || -> Vec<String> {
            let mut files: Vec<_> = std::fs::read_dir(root.join(reports::REPORTS))
                .unwrap()
                .map(|e| e.unwrap().path())
                .collect();
            files.sort();
            files
                .iter()
                .map(|f| std::fs::read_to_string(f).unwrap())
                .collect()
        };
        let before = texts();
        let dir = empty_dir("refresh-partial");
        let list =
            std::env::join_paths([std::path::PathBuf::from(server), dir.join("missing.exe")])
                .unwrap();
        assert!(refresh(root, Some(list)).is_err());
        assert_eq!(texts(), before);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
