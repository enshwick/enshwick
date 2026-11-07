//! `cargo enshwick pin`: ports the pinned rows of one game build to the next.
//!
//! Given the executable the current rows were made for and a new one, it
//! re-finds every pinned symbol of the loader's build table and of each
//! mod's `[[package.metadata.enshwick.builds]]` rows in the new executable,
//! checks the constants against the code that uses them, and writes new mod
//! rows in each mod's `Cargo.toml` and the routes' check shapes, with a
//! report of how the loader's entries port. It writes no loader table: the
//! loader finds every build by its anchors and routes. It never guesses:
//! whatever it
//! cannot place for sure is left out, with a note, and listed for a human.
//! With `--patterns` it writes the patterns of one build's table from its own
//! executable. See the README.

use anyhow::{Context as _, anyhow, bail};
mod anchor;
mod checks;
mod data;
mod exe;
mod found;
mod patterns;
mod port;
mod report;
mod row;
mod rows;
mod signature;
mod staged;
mod table;

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use enshwick_gamedata::Resolved;
use enshwick_gamedata::reflect::Fingerprint;
use rustc_hash::FxHashMap;

use self::data::{old_side, snapshot_dir, table_files};
use self::exe::Exe;
use self::found::{ANCHORS, FOUND, found_table};
use self::rows::{Row, Side, guid_hex};
use self::staged::Staged;

/// What `cargo enshwick pin` is given.
#[derive(Debug, clap::Args)]
pub(crate) struct Args {
    /// The executable the current rows were made for.
    #[arg(long, required_unless_present = "patterns")]
    old: Option<PathBuf>,
    /// The new executable.
    #[arg(long, required_unless_present = "patterns")]
    new: Option<PathBuf>,
    /// Writes the code patterns of the table for this executable's build,
    /// taken from the executable itself, instead of porting.
    #[arg(long, conflicts_with_all = ["old", "new", "steam_build", "game_version"])]
    patterns: Option<PathBuf>,
    /// The repository whose rows to port.
    #[arg(long, default_value = ".")]
    repo: PathBuf,
    /// The game data folder outside the repository: the kept builds' tables
    /// in `tables/`, ported when one is for the old build, and the typed
    /// snapshots in `typed/`, where `--write` keeps the new build's.
    #[arg(long, env = enshwick_gamedata::DATA_VAR)]
    data: Option<PathBuf>,
    /// The new build's Steam build id, for the report's name of the ported
    /// table.
    #[arg(long)]
    steam_build: Option<u64>,
    /// The game's own version number on the new build, for the report's
    /// ported table; left out when not given.
    #[arg(long)]
    game_version: Option<u32>,
    /// Inserts each new mod row after its old one in the sources and writes
    /// the refreshed check shapes; no loader table is written.
    #[arg(long)]
    write: bool,
    /// Writes the report here instead of to standard output.
    #[arg(long)]
    report: Option<PathBuf>,
    /// With `--patterns`: builds an entry's pattern around the instruction
    /// at an RVA, `<id>=<pick>`. For a constant, `<rva>` of an instruction
    /// whose displacement or immediate carries the value (`disp:`, `neg:`,
    /// `imm:` or `shl:` before it to choose, `+<n>` or `-<n>` after it for a
    /// value next to it), or `confirm:<rva>` for an instruction that
    /// confirms it; for a symbol, `<rva>` of an instruction that calls,
    /// jumps to or addresses it, `direct`, or `body` (`body:<rva>` to find
    /// it through a call at `rva`) to check its whole function by a hash.
    #[arg(long, requires = "patterns")]
    from: Vec<String>,
    /// With `--patterns`: a file of `--from` picks, one `<id>=<pick>` or
    /// `<id> <pick>` per line, `#` starting a comment.
    #[arg(long, requires = "patterns")]
    from_file: Option<PathBuf>,
}

/// The exit code when the run worked but some entries need a human.
pub(crate) const NEEDS_HUMAN: u8 = 2;

fn read(path: &Path) -> anyhow::Result<Vec<u8>> {
    fs::read(path).with_context(|| path.display().to_string())
}

/// Runs the tool; `Ok(true)` when nothing needs a human.
pub(crate) fn run(args: &Args) -> anyhow::Result<bool> {
    if let Some(exe) = &args.patterns {
        return patterns_only(args, exe);
    }
    let (Some(old_path), Some(new_path)) = (&args.old, &args.new) else {
        bail!("--old and --new are needed");
    };
    let old_bytes = read(old_path)?;
    let new_bytes = read(new_path)?;
    let old = Exe::parse(&old_bytes).with_context(|| old_path.display().to_string())?;
    let new = Exe::parse(&new_bytes).with_context(|| new_path.display().to_string())?;
    let old_fp = old
        .fingerprint()
        .context("the old executable has no fingerprint")?;
    let new_fp = new
        .fingerprint()
        .context("the new executable has no fingerprint")?;
    if args.write && old_fp == new_fp {
        bail!("both executables are the same build; nothing to write");
    }
    let snapshots = snapshot_dir(args)?;
    let mut staged = Staged::default();
    let mut ported = Vec::new();
    let mut sections = port_tables(args, &old, &new, &mut ported)?;
    sections.extend(refresh_checks(args, &old, &new, &ported, &mut staged)?);
    sections.extend(port_mods(args, &old, &new, &mut staged)?);
    if sections.is_empty() {
        bail!("no row is for the old build {}", spell(old_fp));
    }
    // The typed part never costs the table report: a failure is a line of it.
    let compared = old_side(
        args.data.as_deref(),
        ported.first().map(|(side, ..)| *side),
        old_fp,
    )
    .and_then(|side| typed(args, side, &old_bytes, &new_bytes, old_fp, new_fp));
    let (typed_text, typed_settled) = match &compared {
        Ok(typed) => (typed.report.clone(), typed.settled),
        Err(why) => (format!("Typed API: not compared: {why:#}\n"), false),
    };
    let text = format!(
        "{}\n{typed_text}",
        report::render(&spell(old_fp), &spell(new_fp), &sections)
    );
    finish(staged, args.report.as_deref(), &text)?;
    if let (Some(data), Ok(typed)) = (snapshots, &compared) {
        write_typed(args, data, typed, new_fp)?;
    }
    Ok(typed_settled
        && sections
            .iter()
            .flat_map(|s| &s.lines)
            .all(|l| !l.status.needs_human()))
}

/// The typed API's part of a pin: the report of what the new build changed
/// in the covered types, whether all of it settles without a person, and
/// what `--write` keeps of it.
struct Typed {
    report: String,
    settled: bool,
    side: enshwick_gamedata::Side,
    after: enshwick_gamedata::typed::Catalogue,
    renames: enshwick_gamedata::typed::Renames,
}

/// Compares the covered types of the two builds.
fn typed(
    args: &Args,
    side: enshwick_gamedata::Side,
    old: &[u8],
    new: &[u8],
    old_fp: Fingerprint,
    new_fp: Fingerprint,
) -> anyhow::Result<Typed> {
    let before = crate::generate::catalogue(old).context("the old executable")?;
    let after = crate::generate::catalogue(new).context("the new executable")?;
    let known = crate::generate::renames(&args.repo)?;
    let diff = before.diff(&after, &known);
    Ok(Typed {
        report: format!(
            "Typed API, {} build {} to {}:\n{diff}",
            side.as_str(),
            old_fp.age(),
            new_fp.age()
        ),
        settled: !diff.needs_person(),
        side,
        renames: known.clone().with(&diff),
        after,
    })
}

/// With `--write` in the enshwick repository: the new build's snapshot in
/// the game data folder `data`, the renames its diff found in the rename
/// table and, for a server build, the typed API regenerated.
fn write_typed(args: &Args, data: &Path, typed: &Typed, new_fp: Fingerprint) -> anyhow::Result<()> {
    crate::generate::write_snapshot(data, &typed.after, typed.side, new_fp)?;
    let renames = args.repo.join(enshwick_gamedata::typed::RENAMES);
    if typed.renames != crate::generate::renames(&args.repo)? {
        let text = typed.renames.to_toml().context("the rename table")?;
        fs::write(&renames, text).with_context(|| renames.display().to_string())?;
    }
    if typed.side == enshwick_gamedata::Side::Server {
        let steam = args
            .steam_build
            .map_or_else(String::new, |steam| format!(" (Steam {steam})"));
        let build = format!("server build {}{steam}", new_fp.age());
        crate::generate::write_api(&args.repo, &typed.after, &build)?;
    }
    Ok(())
}

/// Writes the report to `report` before the staged files, so a report that
/// cannot be written leaves every file as it was; without `report`, prints it
/// once the files are written.
fn finish(staged: Staged, report: Option<&Path>, text: &str) -> anyhow::Result<()> {
    if let Some(path) = report {
        fs::write(path, text).with_context(|| path.display().to_string())?;
        return staged.commit();
    }
    staged.commit()?;
    let _shown = write!(std::io::stdout(), "{text}");
    Ok(())
}

/// Ports the loader's table for `old`'s build to `new`'s build for the
/// report: the loader finds every build by its anchors and routes, so no
/// table file is written. Each ported table goes to `tables` as its side,
/// the old table and the new one, for the routes' check shapes.
fn port_tables(
    args: &Args,
    old: &Exe<'_>,
    new: &Exe<'_>,
    tables: &mut Vec<(Side, Resolved, Resolved)>,
) -> anyhow::Result<Vec<report::Section>> {
    let (Some(old_fp), Some(new_fp)) = (old.fingerprint(), new.fingerprint()) else {
        return Ok(Vec::new());
    };
    let mut sources = Vec::new();
    for (path, src) in table_files(args.data.as_deref())? {
        let row = table::parse(&src).with_context(|| path.display().to_string())?;
        if matches(&row, old_fp) {
            let source = format!("the table `{}`", relative(&args.repo, &path));
            sources.push((path, src, row, source));
        }
    }
    if let (true, true, Some(old_path)) = (
        sources.is_empty(),
        args.repo.join(ANCHORS).is_file(),
        args.old.as_deref(),
    ) {
        let side = side_of_exe(old_path).with_context(|| {
            format!(
                "{} has no table and its side shows only in its name: keep it as enshrouded_server.exe or enshrouded.exe",
                old_path.display()
            )
        })?;
        let src = found_table(&args.repo, side, old)?;
        let row = table::parse(&src).context("the table found on the old build")?;
        let path = Path::new(table::DIR).join(table_file(side, old_fp));
        sources.push((path, src, row, FOUND.to_owned()));
    }
    let mut sections = Vec::new();
    for (path, src, row, old_values) in sources {
        let target = path.with_file_name(table_file(row.side, new_fp));
        let mut ported = row::port_row(&row, old, new);
        let source =
            enshwick_gamedata::Table::parse(&src).with_context(|| path.display().to_string())?;
        let anchors = row::port_anchors(&row, &mut ported, &source, old, new);
        row::confirm_symbols_by_patterns(&mut ported, &source, new);
        row::confirm_by_patterns(&mut ported, &source, new);
        row::confirm_cells(&mut ported, &source, new);
        let name = new_name(
            row.name.as_deref().unwrap_or_default(),
            args.steam_build,
            new_fp,
        );
        let guid = guid_hex(new_fp.guid());
        let identity = table::Identity {
            guid: &guid,
            age: new_fp.age(),
            name: &name,
            steam_build: args.steam_build,
            game_version: args.game_version,
            new_build: true,
        };
        let (text, patterned) =
            table::render(&src, &row, &identity, &ported.edits, Some(new), &anchors)
                .with_context(|| path.display().to_string())?;
        sections.push(report::Section {
            file: format!("{} (ported, not written)", relative(&args.repo, &target)),
            title: name,
            source: Some(old_values),
            lines: ported.lines,
            text: Some((text.clone(), "toml")),
            unpatterned: patterned.problems,
        });
        let resolved = enshwick_gamedata::Table::parse(&text)
            .with_context(|| target.display().to_string())?
            .resolve();
        tables.push((row.side, source.resolve(), resolved));
    }
    Ok(sections)
}

/// The side the game executable's file name names.
fn side_of_exe(path: &Path) -> Option<Side> {
    let name = path.file_name()?.to_str()?.to_ascii_lowercase();
    match name.as_str() {
        "enshrouded_server.exe" => Some(Side::Server),
        "enshrouded.exe" => Some(Side::Client),
        _ => None,
    }
}

const fn gamedata_side(side: Side) -> enshwick_gamedata::Side {
    match side {
        Side::Server => enshwick_gamedata::Side::Server,
        Side::Client => enshwick_gamedata::Side::Client,
    }
}

/// Refreshes the routes' check shapes over the old and the new build for
/// each table in `ported`; with `--write`, stages the routes file.
fn refresh_checks(
    args: &Args,
    old: &Exe<'_>,
    new: &Exe<'_>,
    ported: &[(Side, Resolved, Resolved)],
    staged: &mut Staged,
) -> anyhow::Result<Vec<report::Section>> {
    if ported.is_empty() {
        return Ok(Vec::new());
    }
    let path = args.repo.join(checks::ROUTES);
    let before = fs::read_to_string(&path).with_context(|| path.display().to_string())?;
    let mut text = before.clone();
    let mut notes = Vec::new();
    for (side, old_table, new_table) in ported {
        let builds = [(old.image(), old_table), (new.image(), new_table)];
        let (refreshed, more) = checks::refresh(&text, *side, &builds)?;
        text = refreshed;
        notes.extend(more);
    }
    let changed = text != before;
    if args.write && changed {
        staged.replace(path, text.clone(), before);
    }
    Ok(vec![report::Section {
        file: checks::ROUTES.to_owned(),
        title: "check shapes".to_owned(),
        source: None,
        lines: Vec::new(),
        text: (!args.write && changed).then_some((text, "toml")),
        unpatterned: notes,
    }])
}

/// Ports the mods' rows for `old`'s build to `new`'s build; with `--write`,
/// stages each source with its new rows.
fn port_mods(
    args: &Args,
    old: &Exe<'_>,
    new: &Exe<'_>,
    staged: &mut Staged,
) -> anyhow::Result<Vec<report::Section>> {
    let (Some(old_fp), Some(new_fp)) = (old.fingerprint(), new.fingerprint()) else {
        return Ok(Vec::new());
    };
    let mut sections = Vec::new();
    for path in mod_sources(&args.repo) {
        let src = fs::read_to_string(&path).with_context(|| path.display().to_string())?;
        let rows = rows::parse(&src).with_context(|| path.display().to_string())?;
        let file = relative(&args.repo, &path);
        let mut inserts = Vec::new();
        for row in rows.iter().filter(|r| matches(r, old_fp)) {
            if rows
                .iter()
                .any(|r| r.side == row.side && matches(r, new_fp))
                && args.write
            {
                bail!("{file} already has a row for the new build");
            }
            let ported = row::port_row(row, old, new);
            let text = row.render(
                &src,
                &guid_hex(new_fp.guid()),
                new_fp.age(),
                None,
                &ported.edits,
            );
            let title = format!("{:?} row", row.side).to_lowercase();
            if args.write {
                let (at, lead) = row.insertion(&src);
                inserts.push((at, format!("{lead}{text}")));
            }
            sections.push(report::Section {
                file: file.clone(),
                title,
                source: None,
                lines: ported.lines,
                text: (!args.write).then_some((text, "toml")),
                unpatterned: Vec::new(),
            });
        }
        if !inserts.is_empty() {
            let mut out = src.clone();
            inserts.sort_by_key(|(at, _)| std::cmp::Reverse(*at));
            for (at, text) in inserts {
                out.insert_str(at, &text);
            }
            staged.replace(path, out, src);
        }
    }
    Ok(sections)
}

/// `--patterns`: the table of the build `exe` is, with every pattern taken
/// from it.
fn patterns_only(args: &Args, exe_path: &Path) -> anyhow::Result<bool> {
    let bytes = read(exe_path)?;
    let exe = Exe::parse(&bytes).with_context(|| exe_path.display().to_string())?;
    let fp = exe
        .fingerprint()
        .context("the executable has no fingerprint")?;
    for (path, src) in table_files(args.data.as_deref())? {
        let row = table::parse(&src).with_context(|| path.display().to_string())?;
        if !matches(&row, fp) {
            continue;
        }
        let identity = table::Identity {
            guid: &row.guid,
            age: row.age,
            name: row.name.as_deref().unwrap_or_default(),
            steam_build: None,
            game_version: None,
            new_build: false,
        };
        let anchors = picks(args, &row, &exe)?;
        let (text, patterned) = table::render(&src, &row, &identity, &[], Some(&exe), &anchors)
            .with_context(|| path.display().to_string())?;
        let mut summary = format!(
            "{}: patterns for {} of {} symbols and {} of {} constants\n",
            relative(&args.repo, &path),
            patterned.symbols.1,
            patterned.symbols.0,
            patterned.constants.1,
            patterned.constants.0,
        );
        for problem in &patterned.problems {
            summary.push_str("  ");
            summary.push_str(problem);
            summary.push('\n');
        }
        if args.write {
            let mut staged = Staged::default();
            staged.replace(path, text, src);
            staged.commit()?;
            let _shown = write!(std::io::stdout(), "{summary}");
        } else {
            let _shown = write!(std::io::stdout(), "{text}");
            let _shown = write!(std::io::stderr(), "{summary}");
        }
        return Ok(patterned.problems.is_empty());
    }
    Err(anyhow!("no table is for the build {}", spell(fp)))
}

/// The anchors `--from` and `--from-file` pick for the entries of `row`, in
/// `exe`.
fn picks(
    args: &Args,
    row: &Row,
    exe: &Exe<'_>,
) -> anyhow::Result<FxHashMap<String, anchor::Anchor>> {
    let mut lines: Vec<String> = args.from.clone();
    if let Some(path) = &args.from_file {
        let text = fs::read_to_string(path).with_context(|| path.display().to_string())?;
        lines.extend(
            text.lines()
                .map(|l| l.split('#').next().unwrap_or_default().trim().to_owned())
                .filter(|l| !l.is_empty()),
        );
    }
    let mut out = FxHashMap::default();
    for line in lines {
        let (id, pick) = line
            .split_once('=')
            .or_else(|| line.split_once(char::is_whitespace))
            .map(|(id, pick)| (id.trim(), pick.trim()))
            .with_context(|| format!("{line:?} is no `<id>=<pick>`"))?;
        let (value, symbol) = if let Some(e) = row.symbols.iter().find(|e| e.id == id) {
            (e.value, true)
        } else if let Some(e) = row.constants.iter().find(|e| e.id == id) {
            (e.value, false)
        } else {
            bail!("{id}: no such entry in the table");
        };
        let anchor =
            anchor::parse_pick(pick, value, symbol, exe).with_context(|| id.to_string())?;
        out.insert(id.to_owned(), anchor);
    }
    Ok(out)
}

fn relative(repo: &Path, path: &Path) -> String {
    path.strip_prefix(repo)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// The file name of the table for a build, as the loader looks for it.
fn table_file(side: Side, fingerprint: Fingerprint) -> String {
    let side = match side {
        Side::Server => "server",
        Side::Client => "client",
    };
    format!(
        "{side}-{}-{}.toml",
        guid_hex(fingerprint.guid()),
        fingerprint.age()
    )
}

/// Every mod's `Cargo.toml` under `mods/`, which may hold build rows.
fn mod_sources(repo: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut dirs = vec![repo.join("mods")];
    while let Some(dir) = dirs.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        let mut found: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
        found.sort();
        for path in found {
            if path.is_dir() {
                if path.file_name().is_none_or(|n| n != "target") {
                    dirs.push(path);
                }
            } else if path.file_name().is_some_and(|n| n == "Cargo.toml") {
                out.push(path);
            }
        }
    }
    out
}

fn matches(row: &Row, fingerprint: Fingerprint) -> bool {
    row.guid == guid_hex(fingerprint.guid()) && row.age == fingerprint.age()
}

fn spell(fingerprint: Fingerprint) -> String {
    format!("{}/{}", guid_hex(fingerprint.guid()), fingerprint.age())
}

/// The loader table's name for the new build: the build id in parentheses
/// replaced by `steam_build`, or by the fingerprint when it is unknown.
fn new_name(old: &str, steam_build: Option<u64>, fingerprint: Fingerprint) -> String {
    let id = steam_build.map_or_else(|| spell(fingerprint), |b| b.to_string());
    match old.rfind(" (") {
        Some(at) if old.ends_with(')') => format!("{} ({id})", &old[..at]),
        _ => format!("{old} ({id})"),
    }
}

#[cfg(test)]
mod tests {
    use super::rows::Table;
    use super::*;

    #[test]
    fn a_report_that_cannot_be_written_leaves_the_tables_unwritten() {
        let dir = std::env::temp_dir().join(format!("enshwick-pin-report-{}", std::process::id()));
        let _old = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let mut staged = Staged::default();
        staged.create(dir.join("new.toml"), "table".into());
        let report = dir.join("missing").join("report.md");
        assert!(finish(staged, Some(&report), "report").is_err());
        assert!(!dir.join("new.toml").exists());
        let mut staged = Staged::default();
        staged.create(dir.join("new.toml"), "table".into());
        let report = dir.join("report.md");
        finish(staged, Some(&report), "report").unwrap();
        assert_eq!(fs::read_to_string(dir.join("new.toml")).unwrap(), "table");
        assert_eq!(fs::read_to_string(report).unwrap(), "report");
        fs::remove_dir_all(&dir).unwrap();
    }

    /// With `--write` the report still holds each ported loader table whole,
    /// since nothing writes it; skipped without `ENSH_SERVER_EXE`.
    #[test]
    fn a_ported_table_stays_in_the_report_with_write() {
        let Some(path) = std::env::var_os("ENSH_SERVER_EXE") else {
            return;
        };
        let bytes = fs::read(&path).unwrap();
        let exe = Exe::parse(&bytes).unwrap();
        let args = Args {
            // A build without a table of its own ports from what the old
            // executable's name says its side is.
            old: Some(path.into()),
            new: None,
            patterns: None,
            repo: Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."),
            data: enshwick_gamedata::data_dir(),
            steam_build: None,
            game_version: None,
            write: true,
            report: None,
            from: Vec::new(),
            from_file: None,
        };
        let sections = port_tables(&args, &exe, &exe, &mut Vec::new()).unwrap();
        assert!(!sections.is_empty());
        for section in &sections {
            assert!(section.text.is_some(), "{}", section.file);
            assert!(
                section.file.ends_with("(ported, not written)"),
                "{}",
                section.file
            );
        }
    }

    #[test]
    fn the_side_comes_from_the_executable_s_name() {
        assert_eq!(
            side_of_exe(Path::new("x/Enshrouded_Server.exe")),
            Some(Side::Server)
        );
        assert_eq!(side_of_exe(Path::new("enshrouded.exe")), Some(Side::Client));
        assert_eq!(side_of_exe(Path::new("game.exe")), None);
    }

    #[test]
    fn the_new_name_carries_the_new_build() {
        let fp = Fingerprint::new([0xab; 16], 9);
        assert_eq!(
            new_name("1.0 server (26083076)", Some(26_100_000), fp),
            "1.0 server (26100000)"
        );
        assert_eq!(
            new_name("1.0 server", None, fp),
            format!("1.0 server ({}/9)", "ab".repeat(16))
        );
    }

    /// The kept tables, when the game data folder is given, and the mods'
    /// rows parse.
    #[test]
    fn the_kept_tables_and_the_repository_rows_parse() {
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let kept = table_files(enshwick_gamedata::data_dir().as_deref()).unwrap();
        let mut tables = Vec::new();
        for (path, src) in &kept {
            let row = table::parse(src).unwrap();
            assert!(!row.symbols.is_empty());
            assert_eq!(
                path.file_name().and_then(|n| n.to_str()),
                Some(table_file(row.side, Fingerprint::new(guid(&row.guid), row.age)).as_str())
            );
            tables.push(row.table);
        }
        for path in mod_sources(&repo) {
            let src = fs::read_to_string(&path).unwrap();
            for row in rows::parse(&src).unwrap() {
                assert!(!row.symbols.is_empty() || !row.constants.is_empty());
                tables.push(row.table);
            }
        }
        assert!(kept.is_empty() || tables.contains(&Table::Loader));
    }

    fn guid(hex: &str) -> [u8; 16] {
        let mut out = [0; 16];
        for (i, byte) in out.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).unwrap();
        }
        out
    }

    /// The raw file offset and size of section `name`.
    fn raw_section(bytes: &[u8], name: &[u8]) -> (usize, usize) {
        let u16_at = |at: usize| usize::from(u16::from_le_bytes([bytes[at], bytes[at + 1]]));
        let u32_at = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
        let pe = u32_at(0x3c);
        let headers = pe + 24 + u16_at(pe + 20);
        (0..u16_at(pe + 6))
            .map(|i| headers + 40 * i)
            .find(|&at| bytes[at..at + 8].starts_with(name))
            .map(|at| (u32_at(at + 20), u32_at(at + 16)))
            .unwrap()
    }

    /// The executable in `ENSHWICK_PIN_EXE` and the rows for its build,
    /// each with its source text.
    fn exe_and_rows() -> (Vec<u8>, Vec<(String, Row)>) {
        let path = std::env::var("ENSHWICK_PIN_EXE").expect("ENSHWICK_PIN_EXE");
        let bytes = fs::read(&path).unwrap();
        let fp = Exe::parse(&bytes).unwrap().fingerprint().unwrap();
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut rows = Vec::new();
        for (_, src) in table_files(enshwick_gamedata::data_dir().as_deref()).unwrap() {
            let row = table::parse(&src).unwrap();
            if matches(&row, fp) {
                rows.push((src, row));
            }
        }
        for file in mod_sources(&repo) {
            let src = fs::read_to_string(&file).unwrap();
            for row in rows::parse(&src).unwrap() {
                if matches(&row, fp) {
                    rows.push((src.clone(), row));
                }
            }
        }
        assert!(!rows.is_empty(), "no row is for {path}");
        (bytes, rows)
    }

    /// Moves all the code of the executable in `ENSHWICK_PIN_EXE` 16 bytes
    /// on, function table included: every symbol in code must follow.
    ///
    /// `ENSHWICK_PIN_EXE=path/to/enshrouded_server.exe cargo test --release
    /// -p cargo-enshwick -- --ignored`.
    #[test]
    #[ignore = "needs a game executable in ENSHWICK_PIN_EXE"]
    fn moved_code_is_followed() {
        const SHIFT: u32 = 16;
        let shift = SHIFT as usize;
        let (bytes, rows) = exe_and_rows();
        let mut moved = bytes.clone();
        let (text, text_len) = raw_section(&bytes, b".text");
        moved[text..text + shift].fill(0xcc);
        moved[text + shift..text + text_len].copy_from_slice(&bytes[text..text + text_len - shift]);
        let (pdata, pdata_len) = raw_section(&bytes, b".pdata");
        for entry in moved[pdata..pdata + pdata_len].as_chunks_mut::<12>().0 {
            for field in entry[..8].as_chunks_mut::<4>().0 {
                let value = u32::from_le_bytes(*field);
                if value != 0 {
                    *field = (value + SHIFT).to_le_bytes();
                }
            }
        }
        let old = Exe::parse(&bytes).unwrap();
        // One function is rewritten: it must not be found anywhere.
        let (code, code_rva) = old.text();
        let erased = rows[0].1.symbols[0].value;
        let erased_fn = old.function_at(erased).unwrap();
        let at = text + shift + usize::try_from(erased_fn.start - code_rva).unwrap();
        let len = usize::try_from(erased_fn.end - erased_fn.start).unwrap();
        moved[at..at + len].fill(0x90);
        let new = Exe::parse(&moved).unwrap();
        let in_code = |rva: u64| (code_rva..code_rva + code.len() as u64).contains(&rva);
        let mut wrong = Vec::new();
        let mut followed = 0;
        for (_, row) in &rows {
            for line in row::port_row(row, &old, &new).lines {
                if line.kind != row::Kind::Symbol || !in_code(line.old) {
                    continue;
                }
                if line.old == erased {
                    assert_eq!(line.status, row::Status::Missing, "{}", line.note);
                    assert!(line.note.contains("check by hand"), "{}", line.note);
                    continue;
                }
                followed += 1;
                if line.new != Some(line.old + u64::from(SHIFT))
                    || line.status != row::Status::Moved
                {
                    wrong.push(format!("{} {}: {}", line.id, line.status, line.note));
                }
            }
        }
        assert!(followed > 0);
        assert!(wrong.is_empty(), "{followed} followed, wrong: {wrong:#?}");
    }

    /// Moves three fields that the loader table reads from code by 8 bytes,
    /// in the displacements its patterns read: porting re-reads each new
    /// offset, and the patterns alone find it too.
    ///
    /// `ENSHWICK_PIN_EXE=path/to/enshrouded_server.exe cargo test --release
    /// -p cargo-enshwick -- --ignored`.
    #[test]
    #[ignore = "needs a game executable in ENSHWICK_PIN_EXE"]
    fn a_moved_field_is_read_on_the_new_build() {
        let (bytes, rows) = exe_and_rows();
        let (src, row) = rows.iter().find(|(_, r)| r.table == Table::Loader).unwrap();
        let source = enshwick_gamedata::Table::parse(src).unwrap();
        let old = Exe::parse(&bytes).unwrap();
        let (code, code_rva) = old.text();
        let (text, _) = raw_section(&bytes, b".text");
        let mut moved = bytes.clone();
        let mut changed = Vec::new();
        for constant in source.constants() {
            let (Some(pattern), Some(operand)) = (constant.pattern(), constant.operand()) else {
                continue;
            };
            if operand.size() != 4
                || operand != enshwick_gamedata::Operand::new(operand.at(), 4).unwrap()
            {
                continue;
            }
            let at =
                text + pattern.find_unique(code).unwrap() + usize::try_from(operand.at()).unwrap();
            let value = constant.value() + 8;
            moved[at..at + 4].copy_from_slice(&u32::try_from(value).unwrap().to_le_bytes());
            changed.push((constant.id().to_owned(), value));
            if changed.len() == 3 {
                break;
            }
        }
        assert_eq!(changed.len(), 3);
        let new = Exe::parse(&moved).unwrap();
        let mut ported = row::port_row(row, &old, &new);
        let anchors = row::port_anchors(row, &mut ported, &source, &old, &new);
        let (new_code, _) = new.text();
        let resolved = source.fall_back(new_code, code_rva, |_| None);
        for (id, value) in &changed {
            let line = ported.lines.iter().find(|l| &l.id == id).unwrap();
            assert_eq!(
                (line.new, line.status),
                (Some(*value), row::Status::Read),
                "{id}"
            );
            assert!(anchors.contains_key(id), "{id}");
            assert_eq!(resolved.constant(id), Some(*value), "{id}");
        }
    }

    /// Ports every row of one executable's build to the same executable, as
    /// the tool does with `--old` and `--new`: every symbol and check must
    /// map to itself, and no constant may be flagged or change.
    ///
    /// `ENSHWICK_PIN_EXE=path/to/enshrouded_server.exe cargo test --release
    /// -p cargo-enshwick -- --ignored`.
    #[test]
    #[ignore = "needs a game executable in ENSHWICK_PIN_EXE"]
    fn a_build_ports_to_itself() {
        let (bytes, rows) = exe_and_rows();
        let exe = Exe::parse(&bytes).unwrap();
        let mut ported = 0;
        let mut wrong = Vec::new();
        for (src, row) in &rows {
            let mut result = row::port_row(row, &exe, &exe);
            if row.table == Table::Loader {
                let source = enshwick_gamedata::Table::parse(src).unwrap();
                row::port_anchors(row, &mut result, &source, &exe, &exe);
                row::confirm_by_patterns(&mut result, &source, &exe);
                row::confirm_cells(&mut result, &source, &exe);
                for cell in source.constants().iter().filter(|c| c.cell().is_some()) {
                    let line = result.lines.iter().find(|l| l.id == cell.id()).unwrap();
                    assert_eq!(line.status, row::Status::Kept, "{}: {}", line.id, line.note);
                }
            }
            for line in &result.lines {
                if line.kind == row::Kind::Constant {
                    assert_ne!(
                        line.status,
                        row::Status::Flagged,
                        "{}: {}",
                        line.id,
                        line.note
                    );
                    assert!(
                        line.new.is_none_or(|new| new == line.old),
                        "{}: {}",
                        line.id,
                        line.note
                    );
                    continue;
                }
                ported += 1;
                if line.new != Some(line.old) || line.status != row::Status::Unchanged {
                    wrong.push(format!("{} {}: {}", line.id, line.status, line.note));
                }
            }
            if row.table == Table::Loader {
                let identity = table::Identity {
                    guid: &row.guid,
                    age: row.age,
                    name: row.name.as_deref().unwrap_or_default(),
                    steam_build: None,
                    game_version: None,
                    new_build: false,
                };
                let (text, _) = table::render(
                    src,
                    row,
                    &identity,
                    &result.edits,
                    None,
                    &FxHashMap::default(),
                )
                .unwrap();
                assert_eq!(&text, src, "the same build renders the same table");
                continue;
            }
            let fp = exe.fingerprint().unwrap();
            let text = row.render(src, &guid_hex(fp.guid()), fp.age(), None, &result.edits);
            assert_eq!(
                text,
                src[row.span.clone()],
                "the same build renders the same row"
            );
        }
        assert!(wrong.is_empty(), "{ported} ported, wrong: {wrong:#?}");
    }
}
