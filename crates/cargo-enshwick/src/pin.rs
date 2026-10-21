//! `cargo enshwick pin`: ports the pinned rows of one game build to the next.
//!
//! Given the executable the current rows were made for and a new one, it
//! re-finds every pinned symbol of the loader's build table and of each mod's
//! `builds:` rows in the new executable, checks the constants against the
//! code that uses them, and writes the new build's table file (with the code
//! patterns that find each entry on a build without a table) and new mod rows
//! in the same Rust format, with a report. It never guesses: whatever it
//! cannot place for sure is left out, with a note, and listed for a human.
//! With `--patterns` it writes the patterns of one build's table from its own
//! executable. See the README.

mod exe;
mod patterns;
mod port;
mod report;
mod row;
mod rows;
mod signature;
mod table;

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use enshwick_loader_reflect::Fingerprint;

use self::exe::Exe;
use self::rows::{Row, guid_hex};

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
    #[arg(long, conflicts_with_all = ["old", "new", "build"])]
    patterns: Option<PathBuf>,
    /// The repository whose rows to port.
    #[arg(long, default_value = ".")]
    repo: PathBuf,
    /// The new build's Steam build id, for the loader table's name and
    /// `build`; needed with `--write`.
    #[arg(long)]
    build: Option<u64>,
    /// Writes the new table file and inserts each new mod row after its old
    /// one in the sources.
    #[arg(long)]
    write: bool,
    /// Writes the report here instead of to standard output.
    #[arg(long)]
    report: Option<PathBuf>,
}

/// The exit code when the run worked but some entries need a human.
pub(crate) const NEEDS_HUMAN: u8 = 2;

fn read(path: &Path) -> Result<Vec<u8>, String> {
    fs::read(path).map_err(|e| format!("{}: {e}", path.display()))
}

/// Runs the tool; `Ok(true)` when nothing needs a human.
pub(crate) fn run(args: &Args) -> Result<bool, String> {
    if let Some(exe) = &args.patterns {
        return patterns_only(args, exe);
    }
    let (Some(old_path), Some(new_path)) = (&args.old, &args.new) else {
        return Err("--old and --new are needed".to_owned());
    };
    let old_bytes = read(old_path)?;
    let new_bytes = read(new_path)?;
    let old = Exe::parse(&old_bytes).map_err(|e| format!("{}: {e}", old_path.display()))?;
    let new = Exe::parse(&new_bytes).map_err(|e| format!("{}: {e}", new_path.display()))?;
    let old_fp = old
        .fingerprint()
        .ok_or("the old executable has no fingerprint")?;
    let new_fp = new
        .fingerprint()
        .ok_or("the new executable has no fingerprint")?;
    if args.write && old_fp == new_fp {
        return Err("both executables are the same build; nothing to write".to_owned());
    }
    let mut sections = port_tables(args, &old, &new)?;
    sections.extend(port_mods(args, &old, &new)?);
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

/// Ports the loader's table for `old`'s build to `new`'s build.
fn port_tables(args: &Args, old: &Exe<'_>, new: &Exe<'_>) -> Result<Vec<report::Section>, String> {
    let (Some(old_fp), Some(new_fp)) = (old.fingerprint(), new.fingerprint()) else {
        return Ok(Vec::new());
    };
    let mut sections = Vec::new();
    for path in table_files(&args.repo)? {
        let src = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let row = table::parse(&src).map_err(|e| format!("{}: {e}", path.display()))?;
        if !matches(&row, old_fp) {
            continue;
        }
        let target = path.with_file_name(table_file(row.side, new_fp));
        if args.write {
            if target.exists() {
                return Err(format!("{} already exists", target.display()));
            }
            if args.build.is_none() {
                return Err("--write needs --build for the loader table".to_owned());
            }
        }
        let ported = row::port_row(&row, old, new);
        let name = new_name(row.name.as_deref().unwrap_or_default(), args.build, new_fp);
        let guid = guid_hex(new_fp.guid());
        let identity = table::Identity {
            guid: &guid,
            age: new_fp.age(),
            name: &name,
            build: args.build,
            new_build: true,
        };
        let (text, _) = table::render(&src, &row, &identity, &ported.edits, Some(new))
            .map_err(|e| format!("{}: {e}", path.display()))?;
        if args.write {
            fs::write(&target, &text).map_err(|e| format!("{}: {e}", target.display()))?;
        }
        sections.push(report::Section {
            file: relative(&args.repo, &target),
            title: name,
            lines: ported.lines,
            text: (!args.write).then_some((text, "toml")),
        });
    }
    Ok(sections)
}

/// Ports the mods' rows for `old`'s build to `new`'s build.
fn port_mods(args: &Args, old: &Exe<'_>, new: &Exe<'_>) -> Result<Vec<report::Section>, String> {
    let (Some(old_fp), Some(new_fp)) = (old.fingerprint(), new.fingerprint()) else {
        return Ok(Vec::new());
    };
    let mut sections = Vec::new();
    for path in mod_sources(&args.repo) {
        let src = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let rows = rows::parse(&src).map_err(|e| format!("{}: {e}", path.display()))?;
        let file = relative(&args.repo, &path);
        let mut inserts = Vec::new();
        for row in rows.iter().filter(|r| matches(r, old_fp)) {
            if rows
                .iter()
                .any(|r| r.side == row.side && matches(r, new_fp))
                && args.write
            {
                return Err(format!("{file} already has a row for the new build"));
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
                let (at, indent) = row.insertion(&src);
                inserts.push((at, format!("\n{indent}{text},")));
            }
            sections.push(report::Section {
                file: file.clone(),
                title,
                lines: ported.lines,
                text: (!args.write).then_some((text, "rust")),
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
    Ok(sections)
}

/// `--patterns`: the table of the build `exe` is, with every pattern taken
/// from it.
fn patterns_only(args: &Args, exe_path: &Path) -> Result<bool, String> {
    let bytes = read(exe_path)?;
    let exe = Exe::parse(&bytes).map_err(|e| format!("{}: {e}", exe_path.display()))?;
    let fp = exe
        .fingerprint()
        .ok_or("the executable has no fingerprint")?;
    for path in table_files(&args.repo)? {
        let src = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let row = table::parse(&src).map_err(|e| format!("{}: {e}", path.display()))?;
        if !matches(&row, fp) {
            continue;
        }
        let identity = table::Identity {
            guid: &row.guid,
            age: row.age,
            name: row.name.as_deref().unwrap_or_default(),
            build: None,
            new_build: false,
        };
        let (text, patterned) = table::render(&src, &row, &identity, &[], Some(&exe))
            .map_err(|e| format!("{}: {e}", path.display()))?;
        let summary = format!(
            "{}: patterns for {} of {} symbols and {} of {} constants\n",
            relative(&args.repo, &path),
            patterned.symbols.1,
            patterned.symbols.0,
            patterned.constants.1,
            patterned.constants.0,
        );
        if args.write {
            fs::write(&path, &text).map_err(|e| format!("{}: {e}", path.display()))?;
            let _shown = write!(std::io::stdout(), "{summary}");
        } else {
            let _shown = write!(std::io::stdout(), "{text}");
            let _shown = write!(std::io::stderr(), "{summary}");
        }
        return Ok(true);
    }
    Err(format!("no table is for the build {}", spell(fp)))
}

fn relative(repo: &Path, path: &Path) -> String {
    path.strip_prefix(repo)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// The file name of the table for a build, as the loader looks for it.
fn table_file(side: rows::Side, fingerprint: Fingerprint) -> String {
    let side = match side {
        rows::Side::Server => "server",
        rows::Side::Client => "client",
    };
    format!(
        "{side}-{}-{}.toml",
        guid_hex(fingerprint.guid()),
        fingerprint.age()
    )
}

/// The loader's table files, sorted.
fn table_files(repo: &Path) -> Result<Vec<PathBuf>, String> {
    let dir = repo.join(table::DIR);
    let entries =
        fs::read_dir(&dir).map_err(|_| format!("{} is no Enshwick repository", repo.display()))?;
    let mut out: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("toml"))
        })
        .collect();
    out.sort();
    Ok(out)
}

/// Every mod source, which may hold `builds:` rows.
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
            } else if path.extension().is_some_and(|e| e == "rs") {
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
/// replaced by `build`, or by the fingerprint when it is unknown.
fn new_name(old: &str, build: Option<u64>, fingerprint: Fingerprint) -> String {
    let id = build.map_or_else(|| spell(fingerprint), |b| b.to_string());
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
    fn the_new_name_carries_the_new_build() {
        let fp = Fingerprint::new([0xab; 16], 9);
        assert_eq!(
            new_name("1.0 server (1269579)", Some(1_300_000), fp),
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
        for path in table_files(&repo).unwrap() {
            let row = table::parse(&fs::read_to_string(&path).unwrap()).unwrap();
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
        assert!(tables.contains(&Table::Loader));
        assert!(tables.contains(&Table::Mod));
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
        for file in table_files(&repo).unwrap() {
            let src = fs::read_to_string(&file).unwrap();
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
    /// -p cargo-enshwick -- --ignored`
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

    /// Ports every row of one executable's build to the same executable:
    /// every symbol and check must map to itself.
    ///
    /// `ENSHWICK_PIN_EXE=path/to/enshrouded_server.exe cargo test --release
    /// -p cargo-enshwick -- --ignored`
    #[test]
    #[ignore = "needs a game executable in ENSHWICK_PIN_EXE"]
    fn a_build_ports_to_itself() {
        let (bytes, rows) = exe_and_rows();
        let exe = Exe::parse(&bytes).unwrap();
        let mut ported = 0;
        let mut wrong = Vec::new();
        for (src, row) in &rows {
            let result = row::port_row(row, &exe, &exe);
            for line in &result.lines {
                if line.kind == row::Kind::Constant {
                    assert_ne!(line.status, row::Status::Flagged, "{}", line.id);
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
                    build: None,
                    new_build: false,
                };
                let (text, _) = table::render(src, row, &identity, &result.edits, None).unwrap();
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
