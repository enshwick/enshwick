use cargo_enshwick::pin::rows::Table;

use super::*;

/// The `pin` arguments for `old` to `new` in `repo`, writing nothing.
pub(super) fn args(old: PathBuf, new: PathBuf, repo: PathBuf) -> Args {
    Args {
        old,
        new,
        repo,
        data: None,
        steam_build: None,
        game_version: None,
        write: false,
        report: None,
    }
}

/// The repository root.
pub(super) fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
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
    // A build without a table of its own ports from what the old
    // executable's name says its side is.
    let mut args = args(path.clone().into(), path.into(), repo());
    args.data = enshwick_gamedata::data_dir();
    args.write = true;
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

/// The kept tables parse when the game data folder is given, each named as
/// the loader looks for it.
#[test]
fn the_kept_tables_parse() {
    let kept = table_files(enshwick_gamedata::data_dir().as_deref()).unwrap();
    for (path, src) in &kept {
        let row = table::parse(src).unwrap();
        assert!(!row.symbols.is_empty());
        assert_eq!(row.table, Table::Loader);
        assert_eq!(
            path.file_name().and_then(|n| n.to_str()),
            Some(table_file(row.side, Fingerprint::new(guid(&row.guid), row.age)).as_str())
        );
    }
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

/// The executable in `ENSHWICK_PIN_EXE` and the kept tables for its build,
/// each with its source text.
fn exe_and_rows() -> (Vec<u8>, Vec<(String, Row)>) {
    let path = std::env::var("ENSHWICK_PIN_EXE").expect("ENSHWICK_PIN_EXE");
    let bytes = fs::read(&path).unwrap();
    let fp = Exe::parse(&bytes).unwrap().fingerprint().unwrap();
    let mut rows = Vec::new();
    for (_, src) in table_files(enshwick_gamedata::data_dir().as_deref()).unwrap() {
        let row = table::parse(&src).unwrap();
        if matches(&row, fp) {
            rows.push((src, row));
        }
    }
    assert!(!rows.is_empty(), "no table is for {path}");
    (bytes, rows)
}

/// Moves all the code of the executable in `ENSHWICK_PIN_EXE` 16 bytes
/// on, function table included: every symbol in code must follow.
///
/// `ENSHWICK_PIN_EXE=path/to/enshrouded_server.exe cargo test --release
/// -p xtask --features compat --bin xtask-compat -- --ignored`.
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
            if line.new != Some(line.old + u64::from(SHIFT)) || line.status != row::Status::Moved {
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
/// -p xtask --features compat --bin xtask-compat -- --ignored`.
#[test]
#[ignore = "needs a game executable in ENSHWICK_PIN_EXE"]
fn a_moved_field_is_read_on_the_new_build() {
    let (bytes, rows) = exe_and_rows();
    let (src, row) = &rows[0];
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
        let at = text + pattern.find_unique(code).unwrap() + usize::try_from(operand.at()).unwrap();
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

/// Ports every kept table of one executable's build to the same
/// executable, as `pin` does with `--old` and `--new`: every symbol and
/// check must map to itself, no constant may be flagged or change, and the
/// table renders as it was.
///
/// `ENSHWICK_PIN_EXE=path/to/enshrouded_server.exe cargo test --release
/// -p xtask --features compat --bin xtask-compat -- --ignored`.
#[test]
#[ignore = "needs a game executable in ENSHWICK_PIN_EXE"]
fn a_build_ports_to_itself() {
    let (bytes, rows) = exe_and_rows();
    let exe = Exe::parse(&bytes).unwrap();
    let mut ported = 0;
    let mut wrong = Vec::new();
    for (src, row) in &rows {
        let mut result = row::port_row(row, &exe, &exe);
        let source = enshwick_gamedata::Table::parse(src).unwrap();
        row::port_anchors(row, &mut result, &source, &exe, &exe);
        row::confirm_by_patterns(&mut result, &source, &exe);
        row::confirm_cells(&mut result, &source, &exe);
        for cell in source.constants().iter().filter(|c| c.cell().is_some()) {
            let line = result.lines.iter().find(|l| l.id == cell.id()).unwrap();
            assert_eq!(line.status, row::Status::Kept, "{}: {}", line.id, line.note);
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
    }
    assert!(wrong.is_empty(), "{ported} ported, wrong: {wrong:#?}");
}
