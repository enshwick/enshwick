//! Tests of what a table carried to a build without its own takes.

use crate::{Side, Table, embedded, exact, reflect};

fn tables() -> Vec<Table> {
    embedded()
        .iter()
        .map(|(_, text)| Table::parse(text).unwrap())
        .collect()
}

/// A carried value with a witness counts only where the witness is found
/// once and reads the same value; a row names its witness whole, and only a
/// value its own operand reads alone takes a witness or `one_witness`.
#[test]
fn a_witnessed_value_counts_only_where_its_second_use_agrees() {
    let table = |row: &str| {
        Table::parse(&format!(
            "name = \"t (1)\"\nside = \"server\"\nguid = \"00000000000000000000000000000001\"\nage = 2\n[symbols]\n\"f.s\" = 0x10\n[constants]\n{row}\n"
        ))
    };
    let row = r#""c.off" = { value = 0x30, pattern = "48 8B 41 ?? C3", at = 3, size = 1, witness = "8B 51 ?? 90", witness_at = 2, witness_size = 1 }"#;
    let witnessed = table(row).unwrap();
    let witness = witnessed.constants()[0].witness().unwrap();
    assert_eq!((witness.operand().at(), witness.operand().size()), (2, 1));
    // mov rax,[rcx+0x48]; ret, then mov edx,[rcx+0x48]; nop.
    let agree = [0x48, 0x8b, 0x41, 0x48, 0xc3, 0xcc, 0x8b, 0x51, 0x48, 0x90];
    let carried = witnessed.fall_back(&agree, 0, |_| None);
    assert_eq!(carried.constant("c.off"), Some(0x48));
    let mut differ = agree;
    differ[8] = 0x50;
    let carried = witnessed.fall_back(&differ, 0, |_| None);
    assert_eq!(carried.constant("c.off"), None);
    assert!(carried.missing().iter().any(|id| id == "c.off"));
    let mut unwitnessed = agree;
    unwitnessed[6..10].fill(0xcc);
    assert_eq!(
        witnessed
            .fall_back(&unwitnessed, 0, |_| None)
            .constant("c.off"),
        None
    );
    let alone = table(
        r#""c.off" = { value = 0x30, pattern = "48 8B 41 ?? C3", at = 3, size = 1, one_witness = "no other code holds it" }"#,
    )
    .unwrap();
    assert_eq!(
        alone.constants()[0].one_witness(),
        Some("no other code holds it")
    );
    assert_eq!(
        alone.fall_back(&differ, 0, |_| None).constant("c.off"),
        Some(0x48)
    );
    for refused in [
        r#""c.off" = { value = 0x30, pattern = "48 8B 41 ?? C3", at = 3, size = 1, witness = "8B 51 ?? 90", witness_at = 2 }"#,
        r#""c.off" = { value = 0x30, pattern = "48 8B 41 ?? C3", at = 3, size = 1, witness_at = 2 }"#,
        r#""c.off" = { value = 0x30, pattern = "48 8B 41 ?? C3", at = 3, size = 1, witness_size = 1 }"#,
        r#""c.off" = { value = 0x30, pattern = "48 8B 41 ?? C3", at = 3, size = 1, witness = "8B 51 ?? 90", witness_at = 2, witness_size = 1, one_witness = "x" }"#,
        r#""c.off" = { value = 0x30, pattern = "48 8B 41 ?? C3", fixed = true, one_witness = "x" }"#,
        r#""c.off" = { value = 0x30, pattern = "48 8B 41 ?? C3", at = 3, size = 1, via = "f.s", witness = "8B 51 ?? 90", witness_at = 2, witness_size = 1 }"#,
    ] {
        let error = table(refused).unwrap_err();
        assert!(error.to_string().contains("witness"), "{refused}: {error}");
    }
}

/// Every value a carried pattern's operand reads alone and no route of its
/// side finds has a witness, or says why it has none.
#[test]
fn every_held_operand_value_has_a_witness_or_a_reason() {
    let routes = crate::Routes::embedded().unwrap();
    let bare: Vec<String> = tables()
        .iter()
        .flat_map(|table| {
            table
                .constants()
                .iter()
                .filter(|c| {
                    c.carried()
                        && c.operand().is_some()
                        && c.via().is_none()
                        && c.cell().is_none()
                        && !routes.routes(c.id(), table.side())
                        && c.witness().is_none()
                        && c.one_witness().is_none_or(|why| why.trim().is_empty())
                })
                .map(|c| format!("{}: {}", table.name(), c.id()))
                .collect::<Vec<_>>()
        })
        .collect();
    assert!(bare.is_empty(), "{bare:#?}");
}

/// On every executable `ENSH_SERVER_EXE`, `ENSH_CLIENT_EXE` and the opt-in
/// `ENSH_EXES` path list name (each of its sides by its variable, in the
/// list a server when its file is `enshrouded_server.exe`), each other embedded table of its side carried
/// to it finds no entry at another value than its own table pins: what a
/// carry cannot confirm is missing, never wrong.
#[test]
#[cfg_attr(miri, ignore = "reading a game executable takes hours under Miri")]
fn a_carried_table_finds_no_wrong_value_on_any_kept_build() {
    let mut paths: Vec<(std::path::PathBuf, Side)> = [
        ("ENSH_SERVER_EXE", Side::Server),
        ("ENSH_CLIENT_EXE", Side::Client),
    ]
    .into_iter()
    .filter_map(|(var, side)| Some((std::env::var_os(var)?.into(), side)))
    .collect();
    if let Some(list) = std::env::var_os("ENSH_EXES") {
        paths.extend(std::env::split_paths(&list).map(|path| {
            let server = path
                .file_name()
                .is_some_and(|name| name.eq_ignore_ascii_case("enshrouded_server.exe"));
            (path, if server { Side::Server } else { Side::Client })
        }));
    }
    let tables = tables();
    let mut wrong = Vec::new();
    let named = !paths.is_empty();
    let mut compared = 0;
    for (path, side) in paths {
        let bytes = std::fs::read(&path).unwrap();
        let image = reflect::Image::parse(&bytes, reflect::Layout::File, None).unwrap();
        let Some(own) = exact(&tables, side, image.fingerprint().unwrap()) else {
            continue;
        };
        let own = own.resolve();
        let base = image.base();
        let text = image.range(image.section(".text").unwrap());
        let code = image.read(text.start, text.end - text.start).unwrap();
        for other in tables
            .iter()
            .filter(|t| t.side() == side && t.name() != own.name())
        {
            let carried = other.fall_back(code, text.start - base, |rva| image.u32(base + rva));
            compared += 1;
            for (id, value) in carried.constants() {
                if own.constant(id).is_some_and(|pinned| pinned != *value) {
                    wrong.push(format!(
                        "{} on {}: {id} = {value:#x}, pinned {:#x?}",
                        other.name(),
                        own.name(),
                        own.constant(id)
                    ));
                }
            }
            for (id, rva) in carried.symbols() {
                if own.symbol(id).is_some_and(|pinned| pinned != *rva) {
                    wrong.push(format!(
                        "{} on {}: {id} at {rva:#x}, pinned {:#x?}",
                        other.name(),
                        own.name(),
                        own.symbol(id)
                    ));
                }
            }
        }
    }
    assert!(
        !named || compared > 0,
        "no named executable has its own table"
    );
    assert!(
        wrong.is_empty(),
        "{} wrong:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
}
