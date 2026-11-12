//! The table of an old build that has none, from what the loader finds on
//! it: the stand-in that lets a build without a table still port.

use std::fs;
use std::path::Path;

use anyhow::Context as _;

use super::checks;
use super::exe::Exe;
use super::rows::{Side, guid_hex};
use super::table::spelled;

/// The anchors file, relative to the repository.
pub(super) const ANCHORS: &str = "crates/enshwick-gamedata/anchors/patterns.toml";

/// Where the old values come from when the old build has no table.
pub(super) const FOUND: &str = "what the anchors (`anchors/patterns.toml`) and the engine routes (`anchors/routes.toml`) find on the old build, which has no table";

/// The table text of the build `exe` is, from what the loader finds there
/// without one: the anchors of `side` in `repo` carried by their patterns,
/// then the repository's engine routes. An entry keeps its anchor's row, so
/// its pattern ports as a table's would; an entry only a route finds is a
/// bare value. It stands in for the table of an old build that has none:
/// the routes file was last refreshed to hold there, and the new build's
/// values still come from porting the code, not from the routes.
pub(super) fn found_table(repo: &Path, side: Side, exe: &Exe<'_>) -> anyhow::Result<String> {
    use toml_edit::{DocumentMut, Item, value};
    let fingerprint = exe
        .fingerprint()
        .context("the old executable has no fingerprint")?;
    let gamedata = super::gamedata_side(side);
    let path = repo.join(ANCHORS);
    let text = fs::read_to_string(&path).with_context(|| path.display().to_string())?;
    let anchors = enshwick_gamedata::Table::anchors(&text, gamedata)
        .with_context(|| path.display().to_string())?;
    let (code, code_rva) = exe.text();
    let cell = |rva| {
        exe.data(rva, 4)
            .and_then(|bytes| bytes.try_into().ok())
            .map(u32::from_le_bytes)
    };
    let mut found = anchors.fall_back(code, code_rva, cell);
    let anchored = found.clone();
    let path = repo.join(checks::ROUTES);
    let routes = fs::read_to_string(&path).with_context(|| path.display().to_string())?;
    let routes =
        enshwick_gamedata::Routes::parse(&routes).with_context(|| path.display().to_string())?;
    for (id, routed) in routes.find(exe.image(), exe.reflection(), gamedata, &found) {
        found.route(id, routed);
    }
    let doc: DocumentMut = text.parse().context("anchors file")?;
    let name = gamedata.as_str();
    let rows = |kind: &str| {
        doc.get(name)
            .and_then(|s| s.get(kind))
            .and_then(Item::as_table)
    };
    let mut out = DocumentMut::new();
    out["name"] = value(format!("{name} build found by its anchors and routes"));
    out["side"] = value(name);
    out["guid"] = value(guid_hex(fingerprint.guid()));
    out["age"] = value(i64::from(fingerprint.age()));
    out["symbols"] = found_section(rows("symbols"), found.symbols(), "rva", |id| {
        anchored.symbol(id).is_some()
    })?;
    out["constants"] = found_section(rows("constants"), found.constants(), "value", |id| {
        anchored.constant(id).is_some()
    })?;
    Ok(out.to_string())
}

/// A table section of the `found` entries: each its anchor row in `rows`
/// with its value under `key` where its pattern found it (`anchored`), else
/// the bare value, since the row's pattern does not find a value only a
/// route gave; values are spelled as tables spell them.
fn found_section(
    rows: Option<&toml_edit::Table>,
    found: &[(String, u64)],
    key: &str,
    anchored: impl Fn(&str) -> bool,
) -> anyhow::Result<toml_edit::Item> {
    use toml_edit::{InlineTable, Item, Value};
    let mut out = toml_edit::Table::new();
    for (id, number) in found {
        let number: Value = spelled(*number).parse()?;
        let row = rows
            .filter(|_| anchored(id))
            .and_then(|r| r.get(id))
            .and_then(Item::as_inline_table);
        let item = match row {
            Some(row) => {
                let mut with = InlineTable::new();
                with.insert(key, number);
                for (field, v) in row.iter().filter(|(field, _)| *field != key) {
                    with.insert(field, v.clone());
                }
                Value::InlineTable(with)
            }
            None => number,
        };
        out.insert(id, Item::Value(item));
    }
    Ok(Item::Table(out))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::super::{Args, run, side_of_exe, table, table_file};
    use super::*;

    /// The game executables, with their side, that the opt-in `ENSH_EXES`
    /// path list names, then `ENSH_SERVER_EXE` and `ENSH_CLIENT_EXE`; empty
    /// without them.
    fn kept_executables() -> Vec<(Side, PathBuf)> {
        let list = std::env::var_os("ENSH_EXES").unwrap_or_default();
        std::env::split_paths(&list)
            .chain(
                ["ENSH_SERVER_EXE", "ENSH_CLIENT_EXE"]
                    .into_iter()
                    .filter_map(std::env::var_os)
                    .map(PathBuf::from),
            )
            .filter_map(|path| Some((side_of_exe(&path)?, path)))
            .collect()
    }

    #[test]
    fn an_anchored_entry_keeps_its_row_a_routed_one_does_not_and_a_big_value_is_a_string() {
        let doc: toml_edit::DocumentMut = r#"
[server.symbols]
"f.start" = { pattern = "57", rel = 1 }
"f.routed" = { pattern = "56", rel = 1 }
[server.constants]
"f.size" = { value = 0x10, pattern = "48 83 EC ??", at = 3, size = 1 }
"#
        .parse()
        .unwrap();
        let rows = |kind: &str| doc["server"][kind].as_table();
        let symbols = [
            ("f.start".to_owned(), 0x1000),
            ("f.routed".to_owned(), 0x2000),
        ];
        let constants = [
            ("f.size".to_owned(), 0x20),
            ("f.start.bytes".to_owned(), u64::MAX),
        ];
        let anchored = |id: &str| id != "f.routed";
        let mut out = toml_edit::DocumentMut::new();
        out["symbols"] = found_section(rows("symbols"), &symbols, "rva", anchored).unwrap();
        out["constants"] = found_section(rows("constants"), &constants, "value", anchored).unwrap();
        let text = out.to_string();
        assert!(
            text.contains(r#""f.start" = { rva = 0x1000, pattern = "57", rel = 1 }"#),
            "{text}"
        );
        assert!(text.contains(r#""f.routed" = 0x2000"#), "{text}");
        assert!(
            text.contains(
                r#""f.size" = { value = 0x20, pattern = "48 83 EC ??", at = 3, size = 1 }"#
            ),
            "{text}"
        );
        assert!(
            text.contains(r#""f.start.bytes" = "0xffff_ffff_ffff_ffff""#),
            "{text}"
        );
    }

    /// On each kept executable that has a table, the table found by its
    /// anchors and routes reads and agrees with the table on every entry
    /// both have; skipped without `ENSH_EXES` and the game data folder.
    #[test]
    fn the_table_found_on_a_build_agrees_with_its_own() {
        let repo = super::super::tests::repo();
        let Some(data) = enshwick_gamedata::data_dir() else {
            return;
        };
        for (side, path) in kept_executables() {
            let bytes = fs::read(&path).unwrap();
            let exe = Exe::parse(&bytes).unwrap();
            let fp = exe.fingerprint().unwrap();
            let file = data.join(table::DIR).join(table_file(side, fp));
            let Ok(own) = fs::read_to_string(&file) else {
                continue;
            };
            let own = enshwick_gamedata::Table::parse(&own).unwrap().resolve();
            let found = found_table(&repo, side, &exe).unwrap();
            let row = table::parse(&found).unwrap();
            let found = enshwick_gamedata::Table::parse(&found).unwrap().resolve();
            assert!(!row.symbols.is_empty(), "{}", path.display());
            let mut differ = Vec::new();
            for (id, rva) in found.symbols() {
                if own.symbol(id).is_some_and(|pinned| pinned != *rva) {
                    differ.push(id.clone());
                }
            }
            for (id, number) in found.constants() {
                if own.constant(id).is_some_and(|pinned| pinned != *number) {
                    differ.push(id.clone());
                }
            }
            assert!(differ.is_empty(), "{}: {differ:?}", path.display());
        }
    }

    /// Pins from a kept executable whose table is held back to the newest
    /// kept one of its side, in a copy of the repository's anchors without
    /// tables: the report states the old values' source, ports the loader's
    /// entries and refreshes the check shapes; skipped without `ENSH_EXES`.
    #[test]
    fn a_build_without_a_table_still_pins() {
        let kept = kept_executables();
        let Some((side, old)) = kept.first() else {
            return;
        };
        let new = kept
            .iter()
            .rev()
            .find(|(s, _)| s == side)
            .map(|(_, p)| p)
            .unwrap();
        let repo = super::super::tests::repo();
        let held = std::env::temp_dir().join(format!("enshwick-pin-held-{}", std::process::id()));
        let _old = fs::remove_dir_all(&held);
        let anchors = held.join(ANCHORS);
        fs::create_dir_all(anchors.parent().unwrap()).unwrap();
        fs::copy(repo.join(ANCHORS), &anchors).unwrap();
        fs::copy(repo.join(checks::ROUTES), held.join(checks::ROUTES)).unwrap();
        let report = held.join("report.md");
        let args = Args {
            report: Some(report.clone()),
            ..super::super::tests::args(old.clone(), new.clone(), held.clone())
        };
        run(&args).unwrap();
        let text = fs::read_to_string(&report).unwrap();
        fs::remove_dir_all(&held).unwrap();
        assert!(text.contains(&format!("Old values: {FOUND}.")), "{text}");
        assert!(text.contains("anchors/routes.toml: check shapes"), "{text}");
        assert!(text.contains("| symbol |"), "{text}");
    }
}
