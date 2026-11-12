//! `cargo xtask compat pin` and `cargo xtask compat patterns`: the loader's
//! side of porting to a new game build.
//!
//! `pin` re-finds every pinned symbol of the kept table for the old build,
//! or of what the anchors and routes find on it, in the new executable,
//! checks the constants against the code that uses them, refreshes the
//! routes' check shapes and compares the typed API's types, with a report
//! of how the loader's entries port. It writes no loader table: the loader
//! finds every build by its anchors and routes. `patterns` writes the
//! patterns of one kept build's table from its own executable. The porting
//! itself is `cargo enshwick pin`'s, which ports a mod's own rows.

#[path = "pin/checks.rs"]
mod checks;
#[path = "pin/data.rs"]
mod data;
#[path = "pin/found.rs"]
mod found;
#[path = "pin/table.rs"]
mod table;

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, anyhow, bail};
use cargo_enshwick::generate;
use cargo_enshwick::pin::exe::Exe;
use cargo_enshwick::pin::rows::{Row, Side, guid_hex};
use cargo_enshwick::pin::staged::Staged;
use cargo_enshwick::pin::{
    anchor, exe, fingerprints, finish, gamedata_side, matches, patterns, read, relative, report,
    row, rows, side_of_exe, spell,
};
use enshwick_gamedata::Resolved;
use enshwick_gamedata::reflect::Fingerprint;
use rustc_hash::FxHashMap;

use self::data::{old_side, snapshot_dir, table_files};
use self::found::{ANCHORS, FOUND, found_table};

/// What `cargo xtask compat pin` is given.
#[derive(Debug, clap::Args)]
pub(crate) struct Args {
    /// The executable the current table was made for.
    #[arg(long)]
    old: PathBuf,
    /// The new executable.
    #[arg(long)]
    new: PathBuf,
    /// The enshwick repository whose anchors and routes to port.
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
    /// Writes the refreshed check shapes and the new build's typed snapshot,
    /// renames and typed API; no loader table is written.
    #[arg(long)]
    write: bool,
    /// Writes the report here instead of to standard output.
    #[arg(long)]
    report: Option<PathBuf>,
}

/// What `cargo xtask compat patterns` is given.
#[derive(Debug, clap::Args)]
pub(crate) struct PatternsArgs {
    /// The executable whose build's kept table takes its patterns from it.
    exe: PathBuf,
    /// The game data folder outside the repository with the kept builds'
    /// tables in `tables/`.
    #[arg(long, env = enshwick_gamedata::DATA_VAR)]
    data: Option<PathBuf>,
    /// Writes the table back instead of printing it.
    #[arg(long)]
    write: bool,
    /// Builds an entry's pattern around the instruction at an RVA,
    /// `<id>=<pick>`. For a constant, `<rva>` of an instruction whose
    /// displacement or immediate carries the value (`disp:`, `neg:`,
    /// `imm:` or `shl:` before it to choose, `+<n>` or `-<n>` after it for a
    /// value next to it), or `confirm:<rva>` for an instruction that
    /// confirms it; for a symbol, `<rva>` of an instruction that calls,
    /// jumps to or addresses it, `direct`, or `body` (`body:<rva>` to find
    /// it through a call at `rva`) to check its whole function by a hash.
    #[arg(long)]
    from: Vec<String>,
    /// A file of `--from` picks, one `<id>=<pick>` or `<id> <pick>` per
    /// line, `#` starting a comment.
    #[arg(long)]
    from_file: Option<PathBuf>,
}

/// Runs `pin`; `Ok(true)` when nothing needs a human.
pub(crate) fn run(args: &Args) -> anyhow::Result<bool> {
    let old_bytes = read(&args.old)?;
    let new_bytes = read(&args.new)?;
    let old = Exe::parse(&old_bytes).with_context(|| args.old.display().to_string())?;
    let new = Exe::parse(&new_bytes).with_context(|| args.new.display().to_string())?;
    let (old_fp, new_fp) = fingerprints(&old, &new, args.write)?;
    let snapshots = snapshot_dir(args)?;
    let mut staged = Staged::default();
    let mut ported = Vec::new();
    let mut sections = port_tables(args, &old, &new, &mut ported)?;
    sections.extend(refresh_checks(args, &old, &new, &ported, &mut staged)?);
    if sections.is_empty() {
        bail!("no table is for the old build {}", spell(old_fp));
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
    let before = generate::catalogue(old).context("the old executable")?;
    let after = generate::catalogue(new).context("the new executable")?;
    let known = generate::renames(&args.repo)?;
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

/// With `--write`: the new build's snapshot in the game data folder `data`,
/// the renames its diff found in the rename table and, for a server build,
/// the typed API regenerated.
fn write_typed(args: &Args, data: &Path, typed: &Typed, new_fp: Fingerprint) -> anyhow::Result<()> {
    generate::write_snapshot(data, &typed.after, typed.side, new_fp)?;
    let renames = args.repo.join(enshwick_gamedata::typed::RENAMES);
    if typed.renames != generate::renames(&args.repo)? {
        let text = typed.renames.to_toml().context("the rename table")?;
        fs::write(&renames, text).with_context(|| renames.display().to_string())?;
    }
    if typed.side == enshwick_gamedata::Side::Server {
        let steam = args
            .steam_build
            .map_or_else(String::new, |steam| format!(" (Steam {steam})"));
        let build = format!("server build {}{steam}", new_fp.age());
        generate::write_api(&args.repo, &typed.after, &build)?;
    }
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
    if sources.is_empty() && args.repo.join(ANCHORS).is_file() {
        let side = side_of_exe(&args.old).with_context(|| {
            format!(
                "{} has no table and its side shows only in its name: keep it as enshrouded_server.exe or enshrouded.exe",
                args.old.display()
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

/// Runs `patterns`: the table of the build `args.exe` is, with every
/// pattern taken from it; `Ok(true)` when every entry has one.
pub(crate) fn patterns(args: &PatternsArgs) -> anyhow::Result<bool> {
    let bytes = read(&args.exe)?;
    let exe = Exe::parse(&bytes).with_context(|| args.exe.display().to_string())?;
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
            path.display().to_string().replace('\\', "/"),
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
    args: &PatternsArgs,
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
#[path = "pin/tests.rs"]
mod tests;
