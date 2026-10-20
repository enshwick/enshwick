//! `enshwick-pin`: ports the pinned rows of one game build to the next.
//!
//! Given the executable the current rows were made for and a new one, it
//! re-finds every pinned symbol of the loader's build table and of each mod's
//! `builds:` rows in the new executable, checks the constants against the
//! code that uses them, and writes new rows in the same Rust format with a
//! report. It never guesses: whatever it cannot place for sure is left out of
//! the new row, with a note, and listed for a human. See the README.

mod exe;
mod pin;
mod port;
mod report;
mod rows;
mod signature;

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Parser;
use enshwick_reflect::Fingerprint;

use crate::exe::Exe;
use crate::rows::{Row, guid_hex};

/// Ports Enshwick's build rows to a new game build.
#[derive(Debug, Parser)]
#[command(name = "enshwick-pin", version)]
struct Args {
    /// The executable the current rows were made for.
    #[arg(long)]
    old: PathBuf,
    /// The new executable.
    #[arg(long)]
    new: PathBuf,
    /// The repository whose rows to port.
    #[arg(long, default_value = ".")]
    repo: PathBuf,
    /// The new build's Steam build id, for the loader row's name.
    #[arg(long)]
    build: Option<String>,
    /// Inserts each new row after its old one in the sources.
    #[arg(long)]
    write: bool,
    /// Writes the report here instead of to standard output.
    #[arg(long)]
    report: Option<PathBuf>,
}

/// The exit code when the run worked but some entries need a human.
const NEEDS_HUMAN: u8 = 2;

fn main() -> ExitCode {
    match run(&Args::parse()) {
        Ok(clean) => {
            if clean {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(NEEDS_HUMAN)
            }
        }
        Err(error) => {
            let _shown = writeln!(std::io::stderr(), "error: {error}");
            ExitCode::FAILURE
        }
    }
}

/// Runs the tool; `Ok(true)` when nothing needs a human.
fn run(args: &Args) -> Result<bool, String> {
    let old_bytes = fs::read(&args.old).map_err(|e| format!("{}: {e}", args.old.display()))?;
    let new_bytes = fs::read(&args.new).map_err(|e| format!("{}: {e}", args.new.display()))?;
    let old = Exe::parse(&old_bytes).map_err(|e| format!("{}: {e}", args.old.display()))?;
    let new = Exe::parse(&new_bytes).map_err(|e| format!("{}: {e}", args.new.display()))?;
    let old_fp = old
        .fingerprint()
        .ok_or("the old executable has no fingerprint")?;
    let new_fp = new
        .fingerprint()
        .ok_or("the new executable has no fingerprint")?;
    if args.write && old_fp == new_fp {
        return Err("both executables are the same build; nothing to write".to_owned());
    }
    let mut sections = Vec::new();
    for path in sources(&args.repo)? {
        let src = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let rows = rows::parse(&src).map_err(|e| format!("{}: {e}", path.display()))?;
        let file = path
            .strip_prefix(&args.repo)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        let mut inserts = Vec::new();
        for row in rows.iter().filter(|r| matches(r, old_fp)) {
            if rows
                .iter()
                .any(|r| r.side == row.side && matches(r, new_fp))
                && args.write
            {
                return Err(format!("{file} already has a row for the new build"));
            }
            let ported = pin::port_row(row, &old, &new);
            let name = row
                .name
                .as_deref()
                .map(|n| new_name(n, args.build.as_deref(), new_fp));
            let text = row.render(
                &src,
                &guid_hex(new_fp.guid()),
                new_fp.age(),
                name.as_deref(),
                &ported.edits,
            );
            let title = row
                .name
                .clone()
                .unwrap_or_else(|| format!("{:?} row", row.side).to_lowercase());
            if args.write {
                let (at, indent) = row.insertion(&src);
                inserts.push((at, format!("\n{indent}{text},")));
            }
            sections.push(report::Section {
                file: file.clone(),
                title,
                lines: ported.lines,
                text: (!args.write).then_some(text),
            });
        }
        if !inserts.is_empty() {
            let mut out = src.clone();
            inserts.sort_by_key(|(at, _)| std::cmp::Reverse(*at));
            for (at, text) in inserts {
                out.insert_str(at, &text);
            }
            fs::write(&path, out).map_err(|e| format!("{}: {e}", path.display()))?;
        }
    }
    if sections.is_empty() {
        return Err(format!("no row is for the old build {}", spell(old_fp)));
    }
    let text = report::render(&spell(old_fp), &spell(new_fp), &sections);
    match &args.report {
        Some(path) => fs::write(path, &text).map_err(|e| format!("{}: {e}", path.display()))?,
        None => {
            let _shown = write!(std::io::stdout(), "{text}");
        }
    }
    Ok(sections
        .iter()
        .flat_map(|s| &s.lines)
        .all(|l| !l.status.needs_human()))
}

fn matches(row: &Row, fingerprint: Fingerprint) -> bool {
    row.guid == guid_hex(fingerprint.guid()) && row.age == fingerprint.age()
}

fn spell(fingerprint: Fingerprint) -> String {
    format!("{}/{}", guid_hex(fingerprint.guid()), fingerprint.age())
}

/// The loader row's name for the new build: the build id in parentheses
/// replaced by `build`, or by the fingerprint when it is unknown.
fn new_name(old: &str, build: Option<&str>, fingerprint: Fingerprint) -> String {
    let id = build.map_or_else(|| spell(fingerprint), str::to_owned);
    match old.rfind(" (") {
        Some(at) if old.ends_with(')') => format!("{} ({id})", &old[..at]),
        _ => format!("{old} ({id})"),
    }
}

/// The files that may hold rows: the loader's table and every mod source.
fn sources(repo: &Path) -> Result<Vec<PathBuf>, String> {
    let loader = repo.join("crates/enshwick-loader/src/builds.rs");
    if !loader.is_file() {
        return Err(format!("{} is no Enshwick repository", repo.display()));
    }
    let mut out = vec![loader];
    let mods = repo.join("mods");
    let mut dirs = vec![mods];
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
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rows::Table;

    #[test]
    fn the_new_name_carries_the_new_build() {
        let fp = Fingerprint::new([0xab; 16], 9);
        assert_eq!(
            new_name("1.0 server (1269579)", Some("1300000"), fp),
            "1.0 server (1300000)"
        );
        assert_eq!(
            new_name("1.0 server", None, fp),
            format!("1.0 server ({}/9)", "ab".repeat(16))
        );
    }

    #[test]
    fn the_repository_rows_parse() {
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut tables = Vec::new();
        for path in sources(&repo).unwrap() {
            let src = fs::read_to_string(&path).unwrap();
            for row in rows::parse(&src).unwrap() {
                assert!(!row.symbols.is_empty() || !row.constants.is_empty());
                tables.push(row.table);
            }
        }
        assert!(tables.contains(&Table::Loader));
        assert!(tables.contains(&Table::Mod));
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

    /// The executable in `ENSHWICK_PIN_EXE` and the rows for its build.
    fn exe_and_rows() -> (Vec<u8>, Vec<(String, Row)>) {
        let path = std::env::var("ENSHWICK_PIN_EXE").expect("ENSHWICK_PIN_EXE");
        let bytes = fs::read(&path).unwrap();
        let fp = Exe::parse(&bytes).unwrap().fingerprint().unwrap();
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut rows = Vec::new();
        for file in sources(&repo).unwrap() {
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
    /// -p enshwick-pin -- --ignored`
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
            for line in pin::port_row(row, &old, &new).lines {
                if line.kind != pin::Kind::Symbol || !in_code(line.old) {
                    continue;
                }
                if line.old == erased {
                    assert_eq!(line.status, pin::Status::Missing, "{}", line.note);
                    assert!(line.note.contains("check by hand"), "{}", line.note);
                    continue;
                }
                followed += 1;
                if line.new != Some(line.old + u64::from(SHIFT))
                    || line.status != pin::Status::Moved
                {
                    wrong.push(format!("{} {}: {}", line.id, line.status, line.note));
                }
            }
        }
        assert!(followed > 0);
        assert!(wrong.is_empty(), "{followed} followed, wrong: {wrong:#?}");
    }

    /// Ports every row of one executable's build to the same executable:
    /// every symbol and check must map to itself.
    ///
    /// `ENSHWICK_PIN_EXE=path/to/enshrouded_server.exe cargo test --release
    /// -p enshwick-pin -- --ignored`
    #[test]
    #[ignore = "needs a game executable in ENSHWICK_PIN_EXE"]
    fn a_build_ports_to_itself() {
        let (bytes, rows) = exe_and_rows();
        let exe = Exe::parse(&bytes).unwrap();
        let mut ported = 0;
        let mut wrong = Vec::new();
        for (src, row) in &rows {
            let result = pin::port_row(row, &exe, &exe);
            for line in &result.lines {
                if line.kind == pin::Kind::Constant {
                    assert_ne!(line.status, pin::Status::Flagged, "{}", line.id);
                    continue;
                }
                ported += 1;
                if line.new != Some(line.old) || line.status != pin::Status::Unchanged {
                    wrong.push(format!("{} {}: {}", line.id, line.status, line.note));
                }
            }
            let fp = exe.fingerprint().unwrap();
            let text = row.render(
                src,
                &guid_hex(fp.guid()),
                fp.age(),
                row.name.as_deref(),
                &result.edits,
            );
            assert_eq!(
                text,
                src[row.span.clone()],
                "the same build renders the same row"
            );
        }
        assert!(wrong.is_empty(), "{ported} ported, wrong: {wrong:#?}");
    }
}
