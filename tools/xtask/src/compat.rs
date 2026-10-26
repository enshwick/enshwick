//! What works on a game build.
//!
//! Given a game executable, it takes the build table the loader would take
//! (the one made for the build, or the newest table of its side carried over
//! by patterns), checks it against the executable, and reports which of the
//! loader's features have every entry they name. Given mods, it runs the
//! loader itself on the executable, in a process of its own: the image
//! mapped at its base, the mods resolved and initialised as at a start, and
//! nothing hooked. Each mod that turns off says why, as it would in game.

use std::fmt::Write as _;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use enshwick_gamedata::reflect::{Fingerprint, Image, Layout, Reflection};
use enshwick_gamedata::{self as tables, Kind, PartState, Resolved, Table};

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
    /// Takes the table by patterns even when one is made for this build,
    /// to see what the patterns alone carry.
    #[arg(long)]
    patterns: bool,
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
    about = "What works on a game build: which loader features have every table entry they name, and how given mods start"
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
}

/// Runs `cargo xtask compat` with `args`; whether everything works.
pub(crate) fn main(args: &[String]) -> Result<bool, String> {
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
        (None, Some(args)) => run(&args),
        (None, None) => Err("name a game executable; --help lists the options".to_owned()),
    }
}

/// The game build as the loader would see it.
struct Build {
    side: tables::Side,
    fingerprint: Fingerprint,
    base: u64,
    image: Image<'static>,
    reflection: Reflection<'static>,
    code: &'static [u8],
    code_rva: u64,
    /// The table the loader would use; `None` leaves the game vanilla.
    table: Option<(Resolved, Table)>,
    /// The attribute slots read from the game's data, or why none were.
    slots: Result<Slots, String>,
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

/// Reads the executable and takes its table; the bytes live for the
/// process.
fn build(exe: &Path, patterns: bool) -> Result<Build, String> {
    let side = side_of(exe)?;
    let bytes: &'static [u8] = std::fs::read(exe)
        .map_err(|e| format!("{}: {e}", exe.display()))?
        .leak();
    let image = Image::parse(bytes, Layout::File, None).map_err(|e| e.to_string())?;
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
    let all = known_tables()?;
    let exact = tables::exact(&all, side, fingerprint).filter(|_| !patterns);
    let mut table = match exact {
        Some(table) => Some((table.resolve(), table.clone())),
        None => tables::newest(&all, side)
            .map(|table| (table.fall_back(code, range.start - base), table.clone())),
    };
    let slots = match &mut table {
        Some((resolved, _)) => supply_slots(exe, &reflection, resolved),
        None => Ok(Slots::default()),
    };
    Ok(Build {
        side,
        fingerprint,
        base,
        image,
        reflection,
        code,
        code_rva: range.start - base,
        table,
        slots,
    })
}

/// Gives `table` the attribute slots the game's archive beside `exe`
/// holds, as the loader does at start.
#[cfg(windows)]
fn supply_slots(
    exe: &Path,
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
    let dir = exe
        .parent()
        .filter(|d| !d.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let name = exe.file_stem().and_then(|n| n.to_str()).unwrap_or_default();
    let archive = enshwick_gamedata::archive::Archive::open(dir, name).ok_or_else(|| {
        format!("the game's archive {name}.kfc beside the executable does not read")
    })?;
    let found = enshwick_loader::attribute_slots::read(&archive, reflection)?;
    for (id, attribute) in &wanted {
        let Some(slot) = found.get(attribute).and_then(|&s| u64::try_from(s).ok()) else {
            continue;
        };
        slots.found += 1;
        if let Some(pinned) = table.supply(id, slot)
            && pinned != slot
        {
            slots.differ.push(format!(
                "`{id}`: the game's data puts {attribute} in slot {slot}, the table in {pinned}"
            ));
        }
    }
    Ok(slots)
}

#[cfg(not(windows))]
fn supply_slots(_: &Path, _: &Reflection<'_>, _: &mut Resolved) -> Result<Slots, String> {
    Err("only a Windows build reads the game's archive".to_owned())
}

/// The tables the loader embeds.
fn known_tables() -> Result<Vec<Table>, String> {
    tables::embedded()
        .iter()
        .map(|(file, text)| Table::parse(text).map_err(|e| format!("{file}: {e}")))
        .collect()
}

fn spell(fingerprint: Fingerprint) -> String {
    let guid: String = fingerprint.guid().iter().fold(String::new(), |mut out, b| {
        let _infallible = write!(out, "{b:02x}");
        out
    });
    format!("{guid}/{}", fingerprint.age())
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
    let build = build(&args.exe, args.patterns)?;
    let mut out = String::new();
    let _w = writeln!(out, "# Compatibility report\n");
    let _w = writeln!(
        out,
        "`{}`: the {} build `{}`.\n",
        args.exe.file_name().and_then(|n| n.to_str()).unwrap_or("?"),
        build.side.as_str(),
        spell(build.fingerprint)
    );
    let Some((table, source)) = &build.table else {
        let _w = writeln!(
            out,
            "No build table of this side exists: the loader leaves the game vanilla and no mod runs."
        );
        emit(args, &out)?;
        return Ok(false);
    };
    let mut works = true;
    match table.kind() {
        Kind::Exact => {
            let _w = writeln!(
                out,
                "## Build table\n\n`{}`, made for this build.\n",
                table.name()
            );
            let mut problems = check_exact(&build, source);
            if let Ok(slots) = &build.slots {
                problems.extend(slots.differ.iter().cloned());
            }
            if problems.is_empty() {
                let _w = writeln!(
                    out,
                    "Its systems match reflection and every pattern and expected bytes match where it pins them.\n"
                );
            } else {
                works = false;
                let _w = writeln!(out, "It does not match this executable:\n");
                for problem in problems {
                    let _w = writeln!(out, "- {problem}");
                }
                out.push('\n');
            }
        }
        Kind::Fallback => {
            works = args.patterns && table.missing().is_empty();
            let how = if args.patterns {
                "As asked, the loader's newest table of this side,"
            } else {
                "None is made for this build. The loader takes"
            };
            let _w = writeln!(
                out,
                "## Build table\n\n{how} `{}`, finds its entries by their patterns: {} found, {} missing.\n",
                source.name(),
                table.symbols().len() + table.constants().len(),
                table.missing().len()
            );
            if !args.patterns {
                let _w = writeln!(out, "Write this build's table with `cargo enshwick pin`.\n");
            }
            let _w = writeln!(
                out,
                "Patterns find {} of {} symbols and {} of {} constants.\n",
                table.symbols().len(),
                source.symbols().len(),
                table.constants().len() - build.slots.as_ref().map_or(0, |s| s.found),
                source.constants().len()
            );
            if !table.missing().is_empty() {
                let _w = writeln!(out, "Missing: {}.\n", code_list(table.missing()));
            }
        }
    }
    match &build.slots {
        Ok(slots) if slots.named > 0 => {
            let _w = writeln!(
                out,
                "The game's data beside the executable gives {} of the table's {} attribute slots.\n",
                slots.found, slots.named
            );
        }
        Ok(_) => {}
        Err(why) => {
            let _w = writeln!(out, "Attribute slots: {why}.\n");
        }
    }
    write_features(&mut out, table);
    works &= write_mods(&mut out, args)?;
    emit(args, &out)?;
    Ok(works)
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

/// What does not match in a table made for this build: a system the
/// reflection puts elsewhere, a pattern that no longer matches where its
/// symbol is pinned, or expected bytes that differ.
fn check_exact(build: &Build, table: &Table) -> Vec<String> {
    let mut problems = Vec::new();
    for (name, rva) in table.checks() {
        let entry = build
            .reflection
            .system(name)
            .map_or(0, enshwick_gamedata::reflect::System::entry);
        if entry != build.base + rva {
            problems.push(format!(
                "system `{name}` is not where the table says: nothing loads"
            ));
        }
    }
    let resolved = table.resolve();
    for symbol in table.symbols() {
        if symbol.pattern().is_some()
            && symbol.find(build.code, build.code_rva) != Some(symbol.rva())
        {
            problems.push(format!(
                "the pattern of `{}` does not find it once where it is pinned",
                symbol.id()
            ));
        }
        if let Some(want) = resolved.constant(&format!("{}.bytes", symbol.id())) {
            let have = build.image.u64(build.base + symbol.rva());
            if have != Some(want) {
                problems.push(format!(
                    "`{}` does not start with its expected bytes",
                    symbol.id()
                ));
            }
        }
    }
    for constant in table.constants() {
        if constant.pattern().is_some()
            && table.find_pinned(constant, build.code, build.code_rva) != Some(constant.value())
        {
            problems.push(format!(
                "the pattern of `{}` does not find its value once",
                constant.id()
            ));
        }
    }
    problems
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
        .args(["compat", "run", "--exe"])
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

    #[cfg(windows)]
    #[test]
    fn features_name_what_the_build_lacks() {
        let all = known_tables().unwrap();
        let server = tables::newest(&all, tables::Side::Server).unwrap();
        let exact = server.resolve().parts(MODULE_IDS);
        assert!(!exact.is_empty());
        assert!(
            exact.iter().all(|f| f.missing.is_empty()),
            "an exact table lacks nothing"
        );
        let nothing = server.fall_back(&[0; 64], 0x1000);
        let fallback = nothing.parts(MODULE_IDS);
        assert!(fallback.iter().all(|f| f.missing.len() == f.named));
    }

    #[cfg(windows)]
    #[test]
    fn a_carried_table_does_not_miss_what_only_its_own_build_holds() {
        let all = known_tables().unwrap();
        let client = tables::newest(&all, tables::Side::Client).unwrap();
        assert!(
            client
                .resolve()
                .constant("net.provider_join_slot")
                .is_some()
        );
        let carried = client.fall_back(&[0; 64], 0x1000);
        for feature in carried.parts(MODULE_IDS) {
            assert!(
                !feature.missing.contains(&"net.provider_join_slot"),
                "{}",
                feature.name
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
