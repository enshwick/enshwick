//! What works on a game build.
//!
//! Given a game executable, it finds the loader's entries there as the
//! loader does at its start (the anchors of its side carried by their
//! patterns, then the engine routes) and reports which of the loader's
//! features have every entry they name. Given mods, it runs the loader
//! itself on the executable, in a process of its own: the image mapped at
//! its base, the mods resolved and initialised as at a start, and nothing
//! hooked. Each mod that turns off says why, as it would in game.

#![forbid(unsafe_code)]

use std::fmt::Write as _;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use enshwick_gamedata::reflect::{Fingerprint, Image, Layout, Reflection};
use enshwick_gamedata::{self as tables, PartState, Resolved, Table};

#[path = "compat/report.rs"]
mod report;
#[path = "reports.rs"]
mod reports;
#[cfg(all(test, windows))]
use report::report_data;
use report::{data, refresh};

/// Each part of the loader with the build table ids it names; only a
/// Windows build carries the loader.
#[cfg(windows)]
const MODULE_IDS: &[(&str, &[&str])] = enshwick_loader::TABLE_IDS;
#[cfg(not(windows))]
const MODULE_IDS: &[(&str, &[&str])] = &[];

/// What the tool is given.
#[derive(Debug, clap::Args)]
pub(crate) struct Args {
    /// The game executable: `enshrouded_server.exe` or `enshrouded.exe`.
    exe: PathBuf,
    /// Mod DLLs, or folders whose `*.dll` files are mods, to start against
    /// the build; starting them runs their code, so only DLLs you trust.
    #[arg(long)]
    mods: Vec<PathBuf>,
    /// Writes the report here instead of to standard output.
    #[arg(long)]
    report: Option<PathBuf>,
    /// Finds the entries by the anchors' patterns alone, without the engine
    /// routes, to see what the patterns carry.
    #[arg(long)]
    patterns: bool,
    /// Also writes what the loader finds on the build here, as TOML: the
    /// build's compatibility report data; a folder takes it as
    /// `<side>-<guid>-<age>.toml`.
    #[arg(long)]
    data: Option<PathBuf>,
    /// The build's Steam build id, when no Steam appmanifest beside the
    /// executable or two folders up states it.
    #[arg(long)]
    steam_build: Option<u64>,
    /// The game's own version number, when no game archive beside the
    /// executable states it.
    #[arg(long)]
    game_version: Option<u64>,
}

/// What the hidden `run` step is given: a game folder prepared with
/// the mods to start.
#[derive(Debug, clap::Args)]
pub(crate) struct RunArgs {
    /// The game executable.
    #[arg(long)]
    exe: PathBuf,
    /// The prepared game folder.
    #[arg(long)]
    game: PathBuf,
    /// As for `compat`.
    #[arg(long)]
    patterns: bool,
}

/// The exit code when the run worked but something does not.
pub(crate) const BROKEN: u8 = 2;

/// The compat task's command line.
#[derive(Debug, clap::Parser)]
#[command(
    name = "cargo xtask compat",
    about = "What works on a game build: which loader features find every entry they name, and how given mods start",
    args_conflicts_with_subcommands = true
)]
struct Cli {
    #[command(flatten)]
    args: Option<Args>,
    #[command(subcommand)]
    step: Option<Step>,
}

/// A step the task runs in a process of its own.
#[derive(Debug, clap::Subcommand)]
enum Step {
    /// Runs the loader on a prepared game folder.
    #[command(hide = true)]
    Run(RunArgs),
    /// Writes the report data of each executable the `ENSH_EXES` path list
    /// names into the compatibility reports, keeping the Steam build and
    /// game version a report states where no file beside the executable does.
    Refresh,
}

/// The repository root.
const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match compat(&args) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(BROKEN),
        Err(error) => {
            let _shown = writeln!(std::io::stderr(), "error: {error}");
            ExitCode::FAILURE
        }
    }
}

/// Runs `cargo xtask compat` with `args`; whether everything works.
fn compat(args: &[String]) -> Result<bool, String> {
    use clap::Parser as _;
    let cli = match Cli::try_parse_from(
        std::iter::once("cargo xtask compat".to_owned()).chain(args.iter().cloned()),
    ) {
        Ok(cli) => cli,
        Err(error) if !error.use_stderr() => {
            error.print().map_err(|e| e.to_string())?;
            return Ok(true);
        }
        Err(error) => return Err(error.to_string()),
    };
    match (cli.step, cli.args) {
        (Some(Step::Run(args)), _) => run_loader(&args).map(|()| true),
        (Some(Step::Refresh), _) => {
            refresh(Path::new(ROOT), std::env::var_os("ENSH_EXES")).map(|()| true)
        }
        (None, Some(args)) => run(&args),
        (None, None) => Err("name a game executable; --help lists the options".to_owned()),
    }
}

/// The game build as the loader would see it.
struct Build {
    side: tables::Side,
    fingerprint: Fingerprint,
    /// The build's name as the loader logs it.
    name: String,
    /// The Steam build id, where known.
    steam: Option<u64>,
    /// The game's own version number, where known.
    game_version: Option<u64>,
    /// The anchors of the side: the patterns that find what no route finds.
    anchors: Table,
    /// What the loader finds on the build.
    table: Resolved,
    /// How many of its entries the engine routes found.
    routed: usize,
    /// Every entry a route of this side looks for, found or not.
    route_ids: Vec<String>,
    /// The attribute slots read from the game's data, or why none were.
    slots: Result<Slots, String>,
    /// Whether the game's archive file lies beside the executable.
    archive: bool,
}

/// The attribute slots the game's data beside the executable gives.
#[derive(Debug, Default)]
struct Slots {
    /// How many of the table's slots it gives.
    found: usize,
    /// How many the table names.
    named: usize,
    /// Slots the table pins otherwise.
    differ: Vec<String>,
}

fn side_of(exe: &Path) -> Result<tables::Side, String> {
    let name = exe
        .file_name()
        .and_then(|n| n.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    match name.as_str() {
        "enshrouded_server.exe" => Ok(tables::Side::Server),
        "enshrouded.exe" => Ok(tables::Side::Client),
        _ => Err(format!(
            "{} is neither enshrouded_server.exe nor enshrouded.exe",
            exe.display()
        )),
    }
}

/// Reads the executable and finds the loader's entries there as the loader
/// does at its start: the anchors carried by their patterns, the attribute
/// slots of the game's data, then, unless `args.patterns`, the engine routes.
fn build(args: &Args) -> Result<Build, String> {
    let exe = &args.exe;
    let side = side_of(exe)?;
    let bytes = std::fs::read(exe).map_err(|e| format!("{}: {e}", exe.display()))?;
    let image = Image::parse(&bytes, Layout::File, None).map_err(|e| e.to_string())?;
    let fingerprint = image
        .fingerprint()
        .ok_or("the executable has no fingerprint")?;
    let base = image.base();
    let text = image.section(".text").ok_or("no .text section")?;
    let range = image.range(text);
    let code = image
        .read(range.start, range.end - range.start)
        .ok_or(".text is unreadable")?;
    let reflection = Reflection::scan(&image);
    let anchors = tables::anchors(side).map_err(|e| format!("embedded anchors: {e}"))?;
    let mut table = anchors.fall_back(code, range.start - base, |rva| image.u32(base + rva));
    let dir = exe
        .parent()
        .filter(|d| !d.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let name = exe.file_stem().and_then(|n| n.to_str()).unwrap_or_default();
    let archive = enshwick_gamedata::archive::Archive::open(dir, name);
    let slots = supply_slots(archive.as_ref(), name, &reflection, &mut table);
    let routes = tables::Routes::embedded().map_err(|e| e.to_string())?;
    let found = if args.patterns {
        // As the loader without routes: every entry a route finds is missing
        // unless an anchor found it.
        routes
            .ids()
            .filter(|id| routes.routes(id, side))
            .map(|id| (id, None))
            .collect()
    } else {
        routes.find(&image, &reflection, side, &table)
    };
    let (count, route_ids) = route(found, &mut table);
    let steam = args.steam_build.or_else(|| steam_build(dir, side));
    let game_version = args
        .game_version
        .or_else(|| archive.as_ref()?.version()?.split('|').next()?.parse().ok());
    Ok(Build {
        side,
        fingerprint,
        name: build_name(side, steam, game_version, fingerprint),
        steam,
        game_version,
        anchors,
        table,
        routed: count,
        route_ids,
        slots,
        archive: dir.join(format!("{name}.kfc")).is_file(),
    })
}

/// The Steam build the files beside the game folder `dir` state, as the
/// loader reads it.
#[cfg(windows)]
fn steam_build(dir: &Path, side: tables::Side) -> Option<u64> {
    let running = match side {
        tables::Side::Server => enshwick::abi::Side::SERVER,
        tables::Side::Client => enshwick::abi::Side::CLIENT,
    };
    enshwick_loader::builds::steam_build(dir, running).map(enshwick_gamedata::SteamBuild::get)
}

#[cfg(not(windows))]
fn steam_build(_: &Path, _: tables::Side) -> Option<u64> {
    None
}

/// The build's name as the loader logs it: its side, then the Steam build,
/// the game version and the executable's fingerprint, each where known.
fn build_name(
    side: tables::Side,
    steam_build: Option<u64>,
    game_version: Option<u64>,
    fingerprint: Fingerprint,
) -> String {
    let steam = steam_build.map(|b| format!("Steam build {b}, "));
    let version = game_version.map(|v| format!("game {v}, "));
    format!(
        "{}, {}{}executable {}",
        side.as_str(),
        steam.unwrap_or_default(),
        version.unwrap_or_default(),
        spell(fingerprint)
    )
}

/// Takes into `table` what the engine routes `found`, as the loader does at
/// its start, an entry no route found being missing unless an anchor found
/// it: how many entries they found, and every entry they look for.
fn route(found: Vec<(&str, Option<tables::Routed>)>, table: &mut Resolved) -> (usize, Vec<String>) {
    let ids = found.iter().map(|(id, _)| (*id).to_owned()).collect();
    let mut count = 0;
    for (id, routed) in found {
        match table.route(id, routed) {
            tables::Routing::Agrees | tables::Routing::Supplied => count += 1,
            tables::Routing::Unresolved => {
                table.lack(id);
            }
            _ => {}
        }
    }
    (count, ids)
}

/// Gives `table` the attribute slots the game's archive holds, as the
/// loader does at start.
#[cfg(windows)]
fn supply_slots(
    archive: Option<&enshwick_gamedata::archive::Archive>,
    name: &str,
    reflection: &Reflection<'_>,
    table: &mut Resolved,
) -> Result<Slots, String> {
    let wanted = table.attributes().to_vec();
    let mut slots = Slots {
        named: wanted.len(),
        ..Slots::default()
    };
    if wanted.is_empty() {
        return Ok(slots);
    }
    let archive = archive.ok_or_else(|| {
        format!("the game's archive {name}.kfc beside the executable does not read")
    })?;
    let found = enshwick_loader::attribute_slots::read(archive, reflection)?;
    for (id, attribute) in &wanted {
        let Some(slot) = found.get(attribute).and_then(|&s| u64::try_from(s).ok()) else {
            continue;
        };
        slots.found += 1;
        if let Some(pinned) = table.supply(id, slot)
            && pinned != slot
        {
            slots.differ.push(format!(
                "`{id}`: the game's data puts {attribute} in slot {slot}, the anchors in {pinned}"
            ));
        }
    }
    Ok(slots)
}

#[cfg(not(windows))]
fn supply_slots(
    _: Option<&enshwick_gamedata::archive::Archive>,
    _: &str,
    _: &Reflection<'_>,
    _: &mut Resolved,
) -> Result<Slots, String> {
    Err("only a Windows build reads the game's archive".to_owned())
}

fn spell(fingerprint: Fingerprint) -> String {
    format!(
        "{}/{}",
        reports::hex(&fingerprint.guid()),
        fingerprint.age()
    )
}

/// How one mod fared when the loader started it.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct ModOutcome {
    file: String,
    id: String,
    version: String,
    running: bool,
    reason: String,
}

/// The report; `Ok(true)` when everything works.
pub(crate) fn run(args: &Args) -> Result<bool, String> {
    if args.patterns && args.data.is_some() {
        return Err(
            "--data writes what the loader finds with its engine routes, so not with --patterns"
                .to_owned(),
        );
    }
    let build = build(args)?;
    let mut out = String::new();
    let _w = writeln!(out, "# Compatibility report\n");
    let _w = writeln!(
        out,
        "`{}`: {}.\n",
        args.exe.file_name().and_then(|n| n.to_str()).unwrap_or("?"),
        build.name
    );
    let mut works = write_build(&mut out, args, &build);
    match &build.slots {
        Ok(slots) if slots.named > 0 => {
            let _w = writeln!(
                out,
                "The game's data beside the executable gives {} of the anchors' {} attribute slots.\n",
                slots.found, slots.named
            );
            if !slots.differ.is_empty() {
                works = false;
                for differ in &slots.differ {
                    let _w = writeln!(out, "- {differ}");
                }
                out.push('\n');
            }
        }
        Ok(_) => {}
        Err(why) => {
            let _w = writeln!(out, "Attribute slots: {why}.\n");
        }
    }
    if let Some(path) = &args.data {
        let path = if path.is_dir() {
            path.join(tables::file_name(build.side, build.fingerprint))
        } else {
            path.clone()
        };
        let inputs = report::sealing_inputs(Path::new(ROOT))?;
        std::fs::write(&path, data(&build, &inputs)?)
            .map_err(|e| format!("{}: {e}", path.display()))?;
    }
    write_features(&mut out, &build.table);
    works &= write_mods(&mut out, args)?;
    emit(args, &out)?;
    Ok(works)
}

/// The section on what the loader finds on `build`; whether it finds every
/// entry its anchors and routes name.
fn write_build(out: &mut String, args: &Args, build: &Build) -> bool {
    let table = &build.table;
    let entries = table.symbols().len() + table.constants().len();
    let how = if args.patterns {
        "As asked, by the anchors' patterns alone, without the engine routes".to_owned()
    } else {
        format!(
            "{} by the engine routes, the rest by the anchors' patterns",
            build.routed
        )
    };
    let _w = writeln!(
        out,
        "## Build\n\nThe loader finds {entries} entries on this build, {how}; {} missing.\n",
        table.missing().len()
    );
    if !table.missing().is_empty() {
        let _w = writeln!(out, "Missing: {}.\n", code_list(table.missing()));
    }
    let alone: Vec<String> = build
        .anchors
        .constants()
        .iter()
        .filter(|c| c.one_witness().is_some() && table.constant(c.id()).is_some())
        .map(|c| c.id().to_owned())
        .collect();
    if !alone.is_empty() {
        let _w = writeln!(
            out,
            "Taken on one pattern, with no second use of the value to agree: {}.\n",
            code_list(&alone)
        );
    }
    if tables::reported(build.side, build.fingerprint) {
        let _w = writeln!(
            out,
            "A compatibility report of this loader covers the build.\n"
        );
    } else {
        let _w = writeln!(
            out,
            "No compatibility report of this loader covers the build: with `build_patterns = false` the loader keeps it vanilla. `--data {}` writes its report data.\n",
            reports::REPORTS
        );
    }
    table.missing().is_empty()
}

/// The loader features table.
fn write_features(out: &mut String, table: &Resolved) {
    let _w = writeln!(*out, "## Loader features\n");
    if MODULE_IDS.is_empty() {
        let _w = writeln!(
            *out,
            "Only a Windows build of this tool knows the loader's features.\n"
        );
        return;
    }
    let _w = writeln!(*out, "| Feature | Status | Missing |\n| --- | --- | --- |");
    for part in table.parts(MODULE_IDS) {
        let status = match part.state() {
            PartState::Works => "works".to_owned(),
            PartState::Off => "off".to_owned(),
            _ => format!(
                "partly ({} of {})",
                part.named - part.missing.len(),
                part.named
            ),
        };
        let _w = writeln!(
            out,
            "| {} | {status} | {} |",
            part.name,
            code_list(&part.missing)
        );
    }
    out.push('\n');
}

/// The mods table, when there are mods; whether every mod works.
fn write_mods(out: &mut String, args: &Args) -> Result<bool, String> {
    let mut works = true;
    let dlls = dlls(&args.mods)?;
    if !dlls.is_empty() {
        let _w = writeln!(*out, "## Mods\n");
        match start_mods(args, &dlls) {
            Ok(mods) => {
                let _w = writeln!(
                    out,
                    "| Mod | Version | Status | Why |\n| --- | --- | --- | --- |"
                );
                for m in &mods {
                    let (status, why) = verdict(m);
                    works &= status != "off";
                    let name = if m.id.is_empty() { &m.file } else { &m.id };
                    let _w = writeln!(
                        *out,
                        "| {name} | {} | {status} | {} |",
                        m.version,
                        why.replace('|', "/")
                    );
                }
            }
            Err(why) => {
                works = false;
                let _w = writeln!(*out, "The mods could not be started: {why}");
            }
        }
        out.push('\n');
    }
    Ok(works)
}

fn emit(args: &Args, text: &str) -> Result<(), String> {
    if let Some(path) = &args.report {
        return std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()));
    }
    let _shown = write!(std::io::stdout(), "{text}");
    Ok(())
}

fn code_list<S: AsRef<str>>(ids: &[S]) -> String {
    ids.iter()
        .map(|id| format!("`{}`", id.as_ref()))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The DLLs `paths` name: each file, and the `*.dll` in each folder.
fn dlls(paths: &[PathBuf]) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    for path in paths {
        if path.is_dir() {
            let entries =
                std::fs::read_dir(path).map_err(|e| format!("{}: {e}", path.display()))?;
            let mut found: Vec<PathBuf> = entries
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("dll")))
                .collect();
            found.sort();
            out.extend(found);
        } else {
            out.push(path.clone());
        }
    }
    Ok(out)
}

/// Copies the mods among `dlls` into a fresh game folder and runs the
/// loader on it in a process of its own; how each mod fared.
fn start_mods(args: &Args, dlls: &[PathBuf]) -> Result<Vec<ModOutcome>, String> {
    let game = std::env::temp_dir().join(format!("enshwick-compat-{}", std::process::id()));
    let mods = game.join("mods");
    let _old = std::fs::remove_dir_all(&game);
    std::fs::create_dir_all(&mods).map_err(|e| format!("{}: {e}", mods.display()))?;
    for dll in dlls {
        let bytes = std::fs::read(dll).map_err(|e| format!("{}: {e}", dll.display()))?;
        let is_mod =
            memchr::memmem::find(&bytes, enshwick::abi::exports::QUERY.as_bytes()).is_some();
        if let (true, Some(name)) = (is_mod, dll.file_name()) {
            std::fs::write(mods.join(name), &bytes).map_err(|e| e.to_string())?;
        }
    }
    let me = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut command = std::process::Command::new(me);
    command
        .args(["run", "--exe"])
        .arg(&args.exe)
        .arg("--game")
        .arg(&game);
    if args.patterns {
        command.arg("--patterns");
    }
    let output = command.output().map_err(|e| e.to_string())?;
    let _gone = std::fs::remove_dir_all(&game);
    if !output.status.success() {
        return Err(format!(
            "the loader stopped ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    serde_json::from_slice(&output.stdout).map_err(|e| format!("its report does not read: {e}"))
}

/// The status and reason of a mod's outcome.
fn verdict(m: &ModOutcome) -> (&'static str, String) {
    if m.running {
        ("works", String::new())
    } else if m.reason == "skipped: not for this side of the game" {
        ("not on this side", m.reason.clone())
    } else {
        ("off", m.reason.clone())
    }
}

/// The hidden `run` step: the loader's dry run, printed as JSON.
#[cfg(windows)]
pub(crate) fn run_loader(args: &RunArgs) -> Result<(), String> {
    let run = enshwick_loader::dry_run(&args.exe, &args.game, args.patterns)
        .map_err(|e| e.to_string())?;
    let mods: Vec<ModOutcome> = run
        .mods
        .into_iter()
        .map(|m| ModOutcome {
            file: m.file,
            id: m.id,
            version: m.version,
            running: m.running,
            reason: m.reason,
        })
        .collect();
    let json = serde_json::to_string(&mods).map_err(|e| e.to_string())?;
    let _shown = write!(std::io::stdout(), "{json}");
    Ok(())
}

#[cfg(not(windows))]
pub(crate) fn run_loader(_args: &RunArgs) -> Result<(), String> {
    Err("running the loader needs Windows".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refresh_is_a_step_and_any_other_word_an_executable() {
        use clap::Parser as _;
        let parse = |word: &str| Cli::try_parse_from(["cargo xtask compat", word]).unwrap();
        assert!(matches!(parse("refresh").step, Some(Step::Refresh)));
        let named = parse("enshrouded.exe");
        assert!(named.step.is_none());
        assert_eq!(named.args.unwrap().exe, Path::new("enshrouded.exe"));
    }

    #[test]
    fn the_side_comes_from_the_file_name() {
        assert_eq!(
            side_of(Path::new("x/Enshrouded_Server.exe")),
            Ok(tables::Side::Server)
        );
        assert_eq!(
            side_of(Path::new("enshrouded.exe")),
            Ok(tables::Side::Client)
        );
        assert!(side_of(Path::new("game.exe")).is_err());
    }

    #[test]
    fn a_build_is_named_and_filed_by_what_is_known_of_it() {
        let fp = Fingerprint::new([0xab; 16], 7);
        let guid = "ab".repeat(16);
        assert_eq!(
            build_name(tables::Side::Server, Some(26_319_703), Some(1_283_531), fp),
            format!("server, Steam build 26319703, game 1283531, executable {guid}/7")
        );
        assert_eq!(
            build_name(tables::Side::Client, None, None, fp),
            format!("client, executable {guid}/7")
        );
    }

    fn nothing_found(side: tables::Side) -> Resolved {
        tables::anchors(side)
            .unwrap()
            .fall_back(&[0; 64], 0x1000, |_| None)
    }

    #[test]
    fn an_entry_no_route_finds_is_missing_as_the_loader_has_it() {
        let mut table = nothing_found(tables::Side::Server);
        let (routed, ids) = route(
            vec![
                ("x.route_only", None),
                ("x.found", Some(tables::Routed::Constant(8))),
            ],
            &mut table,
        );
        assert_eq!((routed, ids.len()), (1, 2));
        assert!(table.missing().iter().any(|id| id == "x.route_only"));
        assert!(table.missing().iter().all(|id| id != "x.found"));
    }

    #[test]
    fn report_data_is_refused_for_the_patterns_alone() {
        let args = Args {
            exe: PathBuf::from("enshrouded_server.exe"),
            mods: Vec::new(),
            report: None,
            patterns: true,
            data: Some(PathBuf::from("reports")),
            steam_build: None,
            game_version: None,
        };
        assert!(run(&args).unwrap_err().contains("--patterns"));
    }

    /// The attribute slots are unread only when no archive lies beside the
    /// executable; an archive that does not give them leaves them missing.
    #[cfg(windows)]
    #[test]
    fn attribute_slots_are_unread_only_without_an_archive() {
        let side = tables::Side::Server;
        let fingerprint = Fingerprint::new([0xab; 16], 7);
        let build = |archive| Build {
            side,
            fingerprint,
            name: build_name(side, None, None, fingerprint),
            steam: None,
            game_version: None,
            anchors: tables::anchors(side).unwrap(),
            table: nothing_found(side),
            routed: 0,
            route_ids: Vec::new(),
            slots: Err("no slots".to_owned()),
            archive,
        };
        let slot = build(false)
            .table
            .attributes()
            .first()
            .map(|(id, _)| format!("{id:?}"))
            .unwrap();
        let inputs = [0; 32];
        let without = data(&build(false), &inputs).unwrap();
        let unread = without
            .lines()
            .find(|l| l.starts_with("unread = "))
            .unwrap();
        assert!(unread.contains(&slot), "{without}");
        let with = data(&build(true), &inputs).unwrap();
        assert!(!with.contains("unread = "), "{with}");
        let missing = with.lines().find(|l| l.starts_with("missing = ")).unwrap();
        assert!(missing.contains(&slot), "{with}");
    }

    #[cfg(windows)]
    #[test]
    fn features_name_what_a_build_without_code_lacks() {
        for side in [tables::Side::Server, tables::Side::Client] {
            let anchors = tables::anchors(side).unwrap();
            let nothing = anchors.fall_back(&[0; 64], 0x1000, |_| None);
            let parts = nothing.parts(MODULE_IDS);
            assert!(parts.iter().all(|f| f.missing.len() == f.named), "{side:?}");
        }
    }

    /// The `[[feature]]` tables of the report `file` (its `text`, read as
    /// `data`) follow from its own entries and the loader's feature table,
    /// which the seal does not cover, as `cargo xtask compat refresh` would
    /// write them now.
    #[cfg(windows)]
    fn features_agree(file: &str, text: &str, data: &toml::Table) {
        let ids = |key: &str| -> Vec<&str> {
            data.get(key)
                .and_then(toml::Value::as_array)
                .into_iter()
                .flatten()
                .map(|id| id.as_str().unwrap())
                .collect()
        };
        let side = [tables::Side::Server, tables::Side::Client]
            .into_iter()
            .find(|side| data["side"].as_str() == Some(side.as_str()))
            .unwrap();
        let routes = tables::Routes::embedded().unwrap();
        let anchors = tables::anchors(side).unwrap();
        let known = report::known(
            routes.ids().filter(|id| routes.routes(id, side)),
            &anchors,
            &ids("unread"),
        );
        let start = text.find("\n[[feature]]").unwrap_or(text.len());
        assert_eq!(
            &text[start..],
            report::features(&known, &ids("missing")),
            "{file}: the loader's features changed; `cargo xtask compat refresh` rewrites the report"
        );
    }

    /// Each report in `reports/` has the feature tables the loader's
    /// features give it; on each executable the opt-in `ENSH_EXES` path list
    /// names, the report of its build is what this tool writes now, taking
    /// the Steam build and game version the report states where no file
    /// beside the executable does (skipped without it).
    #[cfg(windows)]
    #[test]
    fn every_report_agrees_with_its_build() {
        let dir = &Path::new(ROOT).join(reports::REPORTS);
        let read = |file: &str| {
            std::fs::read_to_string(dir.join(file)).map(|text| text.replace("\r\n", "\n"))
        };
        let mut files: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        files.sort();
        assert!(!files.is_empty());
        let number =
            |data: &toml::Table, key: &str| data.get(key).and_then(toml::Value::as_integer);
        for file in &files {
            let text = read(file).unwrap();
            let data: toml::Table = text.parse().unwrap();
            features_agree(file, &text, &data);
        }
        let Some(list) = std::env::var_os("ENSH_EXES") else {
            return;
        };
        for exe in std::env::split_paths(&list) {
            let mut args = Args {
                exe: exe.clone(),
                mods: Vec::new(),
                report: None,
                patterns: false,
                data: None,
                steam_build: None,
                game_version: None,
            };
            let first = build(&args).unwrap();
            let file = tables::file_name(first.side, first.fingerprint);
            let want = read(&file).unwrap_or_else(|e| panic!("{}: {file}: {e}", exe.display()));
            let stated: toml::Table = want.parse().unwrap();
            let dir = exe.parent().unwrap();
            if dir.join("steamapps").is_dir() {
                let steam = number(&stated, "steam_build").and_then(|n| u64::try_from(n).ok());
                assert_eq!(first.steam, steam, "{}", exe.display());
            }
            let archive = exe.with_extension("kfc");
            if archive.is_file() {
                let version = number(&stated, "game_version").and_then(|n| u64::try_from(n).ok());
                assert_eq!(first.game_version, version, "{}", exe.display());
            }
            let shown = std::env::temp_dir().join(format!("enshwick-compat-{file}.md"));
            args.report = Some(shown.clone());
            let works = run(&args).unwrap();
            let text = std::fs::read_to_string(&shown).unwrap();
            let _gone = std::fs::remove_file(&shown);
            let entries = number(&stated, "entries").unwrap();
            assert!(
                text.contains(&format!("The loader finds {entries} entries")),
                "{text}"
            );
            assert!(
                text.contains("A compatibility report of this loader covers"),
                "{text}"
            );
            assert_eq!(
                text.contains("attribute slots.\n"),
                archive.is_file(),
                "{text}"
            );
            let complete = stated["missing"].as_array().is_some_and(Vec::is_empty)
                && !stated.contains_key("unread");
            assert_eq!(works, complete, "{}", exe.display());
            assert_eq!(
                report_data(dir, &exe, &reports::inputs(Path::new(ROOT)).unwrap()).unwrap(),
                (file, want),
                "{}: `cargo xtask compat refresh` rewrites the report",
                exe.display()
            );
        }
    }

    #[test]
    fn a_mod_off_for_its_side_is_not_broken() {
        let outcome = |running: bool, reason: &str| ModOutcome {
            file: "m.dll".into(),
            id: "m".into(),
            version: "1.0.0".into(),
            running,
            reason: reason.into(),
        };
        assert_eq!(verdict(&outcome(true, "")).0, "works");
        assert_eq!(
            verdict(&outcome(
                false,
                "interaction filter: missing on this game build"
            ))
            .0,
            "off"
        );
        assert_eq!(
            verdict(&outcome(false, "skipped: not for this side of the game")).0,
            "not on this side"
        );
    }
}
