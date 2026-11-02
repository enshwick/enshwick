use super::*;

/// On each client executable, every client table reads where the join
/// copies its target to its stack and reads the address and ports from
/// the copies.
#[test]
fn the_join_target_copies_read_on_each_client() {
    let ids = [
        "join.target_copy",
        "join.target_ip_read",
        "join.target_word_copy",
        "join.target_game_port_read",
        "join.target_query_port_read",
    ];
    let rows = constants_on_executables(&ids);
    let clients: Vec<_> = rows
        .iter()
        .filter(|(table, _)| table.starts_with("ENSH_CLIENT_EXE"))
        .collect();
    if std::env::var("ENSH_CLIENT_EXE").is_ok() {
        assert!(clients.len() >= 2, "{rows:?}");
    }
    for (table, values) in clients {
        assert_eq!(
            values,
            &[Some(0x50), Some(0x58), Some(0x48), Some(0x4c), Some(0x4e)],
            "{table}"
        );
    }
}

/// On each client executable, every client table reads the size of a
/// texture ring's completion record and where it holds its context.
#[test]
fn the_texture_ring_record_reads_on_each_client() {
    let ids = ["textures.ring_record_size", "textures.ring_record_context"];
    let rows = constants_on_executables(&ids);
    let clients: Vec<_> = rows
        .iter()
        .filter(|(table, _)| table.starts_with("ENSH_CLIENT_EXE"))
        .collect();
    if std::env::var("ENSH_CLIENT_EXE").is_ok() {
        assert!(clients.len() >= 2, "{rows:?}");
    }
    for (table, values) in clients {
        assert_eq!(values, &[Some(0x18), Some(0x8)], "{table}");
    }
}

/// On each executable, every table that pins a column grid's pages
/// reads where it holds its pages along x and along z.
#[test]
fn the_grid_depth_reads_on_each_executable() {
    let rows = pinned_constants_on_executables(&["voxel.grid_pages", "voxel.grid_depth"]);
    let unread = executables_without_rows(&rows, &named_executables());
    assert!(unread.is_empty(), "{unread:?}: {rows:?}");
    for (table, values) in rows {
        assert_eq!(values, [Some(0x48), Some(0x4c)], "{table}");
    }
}

/// On each client executable, every client table reads where the
/// slider's config fields end, before its stop list.
#[test]
fn the_slider_extent_reads_on_each_client() {
    let rows = constants_on_executables(&["ui.slider_extent"]);
    let clients: Vec<_> = rows
        .iter()
        .filter(|(table, _)| table.starts_with("ENSH_CLIENT_EXE"))
        .collect();
    if std::env::var("ENSH_CLIENT_EXE").is_ok() {
        assert!(clients.len() >= 2, "{rows:?}");
    }
    for (table, values) in clients {
        assert_eq!(values, &[Some(0xa0)], "{table}");
    }
}

/// On each executable, every table that pins the block rows reads where
/// the builder's box keeps its lowest voxel and its turned size, how far
/// it writes, and how far the undo write reads its record.
#[test]
fn the_block_box_and_undo_record_rows_read_on_each_executable() {
    let rows = pinned_constants_on_executables(&[
        "building.box_min",
        "building.box_size",
        "building.box_extent",
        "voxel.undo_extent",
    ]);
    let unread = executables_without_rows(&rows, &named_executables());
    assert!(unread.is_empty(), "{unread:?}: {rows:?}");
    for (table, values) in rows {
        assert_eq!(
            values,
            [Some(0x20), Some(0x2c), Some(0x50), Some(0x28)],
            "{table}"
        );
    }
}

/// On each client executable, every client table reads how many bytes
/// a key of the texture database's tables takes.
#[test]
fn the_texture_key_size_reads_on_each_client() {
    let rows = constants_on_executables(&["textures.key_size"]);
    let clients: Vec<_> = rows
        .iter()
        .filter(|(table, _)| table.starts_with("ENSH_CLIENT_EXE"))
        .collect();
    if std::env::var("ENSH_CLIENT_EXE").is_ok() {
        assert!(clients.len() >= 2, "{rows:?}");
    }
    for (table, values) in clients {
        assert_eq!(values, &[Some(0x20)], "{table}");
    }
}

/// On each executable, every table reads the bits of a connection
/// handle that name its chat slot, and the bytes a chat line counts
/// beyond its text.
#[test]
fn the_chat_handle_slot_and_line_overhead_read_on_each_executable() {
    let rows = constants_on_executables(&["chat.handle_slot", "chat.line_overhead"]);
    let named = ["ENSH_SERVER_EXE", "ENSH_CLIENT_EXE"]
        .iter()
        .filter(|var| std::env::var(var).is_ok())
        .count();
    assert!(rows.len() >= named * 2, "{rows:?}");
    for (table, values) in rows {
        assert_eq!(values, [Some(0x7f), Some(0x17)], "{table}");
    }
}

/// On each executable, every table reads where the zone size's answer
/// holds its valid byte.
#[test]
fn the_zone_size_valid_byte_reads_on_each_executable() {
    let rows = constants_on_executables(&["balancing.zone_size_valid"]);
    let named = ["ENSH_SERVER_EXE", "ENSH_CLIENT_EXE"]
        .iter()
        .filter(|var| std::env::var(var).is_ok())
        .count();
    assert!(rows.len() >= named * 2, "{rows:?}");
    for (table, values) in rows {
        assert_eq!(values, [Some(0xc)], "{table}");
    }
}

/// On each executable, every table that pins them reads where the sort
/// into bases zeroes its answer's word and reads the entity's position.
#[test]
fn the_sort_answer_and_entity_read_on_each_executable() {
    let rows =
        pinned_constants_on_executables(&["bases.sort_answer_word", "bases.sort_entity_position"]);
    let unread = executables_without_rows(&rows, &named_executables());
    assert!(unread.is_empty(), "{unread:?}: {rows:?}");
    for (table, values) in rows {
        assert_eq!(values, [Some(0x4), Some(0x8)], "{table}");
    }
}

/// On each executable, every table that pins them reads where the bases
/// object keeps its first record and first stock slot, and a slot's
/// stride.
#[test]
fn the_first_record_and_stock_slot_read_on_each_executable() {
    let rows = pinned_constants_on_executables(&["bases.records", "bases.slots", "bases.slot"]);
    let unread = executables_without_rows(&rows, &named_executables());
    assert!(unread.is_empty(), "{unread:?}: {rows:?}");
    for (table, values) in rows {
        assert_eq!(values, [Some(0x30), Some(0x1610), Some(0x58)], "{table}");
    }
}

/// On each executable, every table reads the words of the offer test's
/// out record a refused offer zeroes.
#[test]
fn the_offer_out_record_reads_on_each_executable() {
    let ids = [
        "access.offer_out_u32",
        "access.offer_out_u64",
        "access.offer_out_tail",
    ];
    let rows = constants_on_executables(&ids);
    let named = ["ENSH_SERVER_EXE", "ENSH_CLIENT_EXE"]
        .iter()
        .filter(|var| std::env::var(var).is_ok())
        .count();
    assert!(rows.len() >= named * 2, "{rows:?}");
    for (table, values) in rows {
        assert_eq!(values, [Some(0x4), Some(0x8), Some(0x10)], "{table}");
    }
}

/// An executable whose own table lacks a row still counts as read when
/// a newer table carried onto it reads the row.
#[test]
fn a_carried_row_alone_reads_an_executable() {
    let rows = vec![(
        "ENSH_CLIENT_EXE: 1.0 client (26082781)".to_owned(),
        vec![Some(4)],
    )];
    assert!(executables_without_rows(&rows, &["ENSH_CLIENT_EXE"]).is_empty());
}

#[test]
fn an_executable_with_no_row_is_named() {
    let rows = vec![("ENSH_CLIENT_EXE: own".to_owned(), vec![Some(4)])];
    assert_eq!(
        executables_without_rows(&rows, &["ENSH_SERVER_EXE", "ENSH_CLIENT_EXE"]),
        ["ENSH_SERVER_EXE"]
    );
}

#[test]
fn code_with_no_check_to_offer_never_holds() {
    assert!(!code_holds(&[None, None, None]));
}

#[test]
fn every_check_offered_must_hold() {
    assert!(code_holds(&[Some(true), Some(true), None]));
    assert!(!code_holds(&[Some(false), Some(true), None]));
    assert!(!code_holds(&[Some(true), Some(false), None]));
}

#[test]
fn a_single_check_decides_alone() {
    assert!(code_holds(&[Some(true), None, None]));
    assert!(!code_holds(&[Some(false), None, None]));
    assert!(code_holds(&[None, Some(true), None]));
    assert!(!code_holds(&[None, Some(false), None]));
}

/// A function whose whole code changed fails its check even when its
/// first bytes still match.
#[test]
fn a_changed_body_fails_the_code_whose_head_holds() {
    assert!(code_holds(&[Some(true), None, Some(true)]));
    assert!(!code_holds(&[Some(true), Some(true), Some(false)]));
    assert!(code_holds(&[None, None, Some(true)]));
}

#[test]
fn a_hook_goes_only_where_the_code_is_found_and_holds() {
    assert_eq!(hook_target(Some(0x10), || true), Ok(0x10));
    assert_eq!(hook_target(None, || true), Err(Unhooked::Absent));
    assert_eq!(hook_target(Some(0), || true), Err(Unhooked::Absent));
    assert_eq!(hook_target(Some(0x10), || false), Err(Unhooked::Moved));
}

#[test]
fn the_code_of_an_absent_hook_is_never_read() {
    assert_eq!(
        hook_target(None, || unreachable!("read")),
        Err(Unhooked::Absent)
    );
}

#[test]
fn an_off_line_names_the_feature_the_build_the_entry_and_why() {
    assert_eq!(
        off_line(
            "fog of war",
            "1.0 client (26082781)",
            "map.fog_reveal",
            &Unhooked::Moved.to_string()
        ),
        "fog of war off on 1.0 client (26082781): map.fog_reveal does not read as the build table says"
    );
    assert_eq!(
        off_line(
            "chat lines",
            "b",
            "chat.post",
            &Unhooked::Absent.to_string()
        ),
        "chat lines off on b: chat.post is not in this build"
    );
}

#[test]
fn a_scan_that_reflects_no_type_or_no_system_is_an_unknown_build() {
    assert!(reflection_read(4000, 300));
    assert!(!reflection_read(0, 300));
    assert!(!reflection_read(4000, 0));
    assert!(!reflection_read(0, 0));
}

/// Every symbol the loader hooks offers a check of its code in every
/// table that pins it, so a table pinned wrong turns one feature off
/// instead of patching another function.
#[test]
fn every_hooked_symbol_offers_a_check_of_its_code_in_every_table() {
    let mut unchecked = Vec::new();
    for table in load() {
        let resolved = table.resolve();
        for symbol in crate::pinned::HOOKED {
            let id = symbol.id();
            if resolved.symbol(id).is_none() {
                continue;
            }
            let head = resolved.head(id).is_some();
            let bytes = resolved.constant(&format!("{id}.bytes")).is_some();
            let body = resolved.body(id).is_some();
            if !head && !bytes && !body {
                unchecked.push(format!("{}: {id}", table.name()));
            }
        }
    }
    assert!(unchecked.is_empty(), "no check offered: {unchecked:#?}");
}

/// Every symbol whose code the loader replaces or calls as a whole
/// carries a body over its whole function in every table that pins it,
/// so a change to the game's logic there fails the code check and
/// turns its feature off.
#[test]
fn every_replaced_function_carries_a_body_in_every_table() {
    let mut wrong = Vec::new();
    for table in load() {
        for symbol in crate::pinned::BODIES {
            let id = symbol.id();
            if table
                .symbols()
                .iter()
                .any(|s| s.id() == id && s.body().is_none())
            {
                wrong.push(format!("{}: {id}", table.name()));
            }
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

#[cfg(windows)]
#[test]
fn a_build_names_the_loader_features_it_lacks_once() {
    assert_eq!(embedded(Side::SERVER).lacking(crate::TABLE_IDS), "");
    let tables = load();
    let unknown = Fingerprint::new([0xee; 16], 1);
    let code = [0u8; 64];
    let carried = select(
        &tables,
        Side::SERVER,
        unknown,
        Some((&code, 0x1000)),
        0..0,
        |_| None,
        None,
    )
    .unwrap();
    let lacking = carried.lacking(crate::TABLE_IDS);
    assert!(
        lacking.split(", ").any(|name| name == "altars"),
        "{lacking}"
    );
    assert_eq!(
        carried.lacking(&[]),
        lacking,
        "the first call names the parts"
    );
}

#[test]
fn a_known_build_resolves_its_symbols() {
    let tables = load();
    let server = embedded(Side::SERVER);
    let fingerprint = tables
        .iter()
        .find(|t| t.name() == server.name)
        .unwrap()
        .fingerprint();
    let found = select(
        &tables,
        Side::SERVER,
        fingerprint,
        None,
        0..0,
        |_| None,
        None,
    )
    .unwrap();
    assert_eq!(found.kind, Kind::Exact);
    assert_eq!(found.symbol("npc.lookup"), Some(0x20_3d30));
    assert_eq!(found.symbol("npc.nope"), None);
    assert!(!found.checks().is_empty());
    assert!(
        select(
            &tables,
            Side::CLIENT,
            fingerprint,
            None,
            0..0,
            |_| None,
            None
        )
        .is_none()
    );
}

#[test]
fn an_unknown_build_falls_back_only_when_allowed_and_given_code() {
    let tables = load();
    let unknown = Fingerprint::new([0xee; 16], 1);
    let code = [0u8; 64];
    let found = select(
        &tables,
        Side::SERVER,
        unknown,
        Some((&code, 0x1000)),
        0..0,
        |_| None,
        None,
    )
    .unwrap();
    assert_eq!(found.kind, Kind::Fallback);
    assert_eq!(found.symbol("npc.lookup"), None);
    assert!(found.checks().is_empty());
    assert!(!found.missing().is_empty());
    assert!(select(&tables, Side::SERVER, unknown, None, 0..0, |_| None, None).is_none());
    assert!(
        select(
            &tables,
            Side::BOTH,
            unknown,
            Some((&code, 0x1000)),
            0..0,
            |_| None,
            None
        )
        .is_none()
    );
}

/// A build without a table is named by its own fingerprint and the table it
/// carries, never as the carried table's build, and an absent entry says
/// whether the carried table lacks it or its pattern found nothing.
#[test]
fn a_build_without_a_table_names_itself_and_the_carried_table() {
    let tables = load();
    let base = tables::newest(&tables, enshwick_gamedata::Side::Server).unwrap();
    let code = [0u8; 64];
    let found = select(
        &tables,
        Side::SERVER,
        Fingerprint::new([0xee; 16], 1),
        Some((&code, 0x1000)),
        0..0,
        |_| None,
        None,
    )
    .unwrap();
    assert_eq!(
        found.name,
        format!(
            "build eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee/1, carried table {}",
            base.name()
        )
    );
    let absent = Unhooked::Absent.to_string();
    assert_eq!(
        found.reason("npc.lookup", &absent),
        "is not found by the carried table's patterns"
    );
    assert_eq!(
        found.reason("npc.nope", &absent),
        "is not in the carried table"
    );
    let moved = Unhooked::Moved.to_string();
    assert_eq!(found.reason("npc.lookup", &moved), moved);
    assert_eq!(embedded(Side::SERVER).reason("npc.nope", &absent), absent);
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("enshwick-carried-{name}-{}", std::process::id()));
    let _fresh = std::fs::remove_dir_all(&dir);
    dir
}

fn patterned_code() -> Vec<u8> {
    let tables = load();
    let base = tables::newest(&tables, enshwick_gamedata::Side::Server).unwrap();
    let mut code = vec![0u8; 4096];
    let (pattern, _) = base.symbols().iter().find_map(|s| s.pattern()).unwrap();
    for i in 0..pattern.len() {
        code[16 + i] = if pattern.is_fixed(i) == Some(true) {
            0
        } else {
            0x5a
        };
    }
    code
}

#[test]
fn a_carried_table_is_saved_and_read_back_while_the_code_stays() {
    let tables = load();
    let unknown = Fingerprint::new([0xee; 16], 1);
    let dir = scratch("same");
    let saved = carried_file(&dir, Side::SERVER).unwrap();
    let code = patterned_code();
    let first = select(
        &tables,
        Side::SERVER,
        unknown,
        Some((&code, 0x1000)),
        0..0,
        |_| None,
        Some(&dir),
    )
    .unwrap();
    let backup = enshwick::files::backup_path(&saved);
    let text = std::fs::read_to_string(&saved).unwrap();
    let again = select(
        &tables,
        Side::SERVER,
        unknown,
        Some((&code, 0x1000)),
        0..0,
        |_| None,
        Some(&dir),
    )
    .unwrap();
    assert_eq!(again.table, first.table);
    assert!(
        !backup.exists(),
        "read from the save, not searched and saved again"
    );
    std::fs::write(&saved, format!("{text}none\n")).unwrap();
    let edited = select(
        &tables,
        Side::SERVER,
        unknown,
        Some((&code, 0x1000)),
        0..0,
        |_| None,
        Some(&dir),
    )
    .unwrap();
    assert_eq!(edited.table, first.table, "searched again");
    assert_eq!(std::fs::read_to_string(&saved).unwrap(), text);
    assert!(backup.exists(), "the edited save was replaced");
    std::fs::remove_dir_all(dir).unwrap();
}

/// The stub's hold on the entry point jumps to wherever the stub loaded, so
/// those bytes differ from start to start and must not turn the save away.
#[test]
fn a_carried_table_is_read_back_when_only_the_entry_hold_differs() {
    let tables = load();
    let unknown = Fingerprint::new([0xee; 16], 1);
    let dir = scratch("hold");
    let saved = carried_file(&dir, Side::SERVER).unwrap();
    let hold = 1000..1014;
    let mut code = patterned_code();
    code[hold.clone()].fill(0x11);
    let first = select(
        &tables,
        Side::SERVER,
        unknown,
        Some((&code, 0x1000)),
        hold.clone(),
        |_| None,
        Some(&dir),
    )
    .unwrap();
    code[hold.clone()].fill(0x22);
    let again = select(
        &tables,
        Side::SERVER,
        unknown,
        Some((&code, 0x1000)),
        hold,
        |_| None,
        Some(&dir),
    )
    .unwrap();
    assert_eq!(again.table, first.table);
    assert!(
        !enshwick::files::backup_path(&saved).exists(),
        "read from the save, not searched and saved again"
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_save_for_other_code_or_a_damaged_one_is_replaced() {
    let tables = load();
    let unknown = Fingerprint::new([0xee; 16], 1);
    let dir = scratch("other");
    let saved = carried_file(&dir, Side::SERVER).unwrap();
    let code = patterned_code();
    let base = tables::newest(&tables, enshwick_gamedata::Side::Server).unwrap();
    std::fs::create_dir_all(saved.parent().unwrap()).unwrap();
    std::fs::write(&saved, "not a carried table").unwrap();
    let found = select(
        &tables,
        Side::SERVER,
        unknown,
        Some((&code, 0x1000)),
        0..0,
        |_| None,
        Some(&dir),
    )
    .unwrap();
    assert_eq!(found.table, base.fall_back(&code, 0x1000, |_| None));
    let key = base.carry_key(&code, 0x1000, 0..0);
    assert!(
        std::fs::read_to_string(&saved)
            .unwrap()
            .contains(&key.to_string())
    );
    let other = vec![0u8; 4096];
    let found = select(
        &tables,
        Side::SERVER,
        unknown,
        Some((&other, 0x1000)),
        0..0,
        |_| None,
        Some(&dir),
    )
    .unwrap();
    assert_eq!(found.table, base.fall_back(&other, 0x1000, |_| None));
    let key = base.carry_key(&other, 0x1000, 0..0);
    assert!(
        std::fs::read_to_string(&saved)
            .unwrap()
            .contains(&key.to_string())
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_build_with_its_own_table_saves_nothing() {
    let tables = load();
    let server = embedded(Side::SERVER);
    let fingerprint = tables
        .iter()
        .find(|t| t.name() == server.name)
        .unwrap()
        .fingerprint();
    let dir = scratch("exact");
    let saved = carried_file(&dir, Side::SERVER).unwrap();
    let code = patterned_code();
    let found = select(
        &tables,
        Side::SERVER,
        fingerprint,
        Some((&code, 0x1000)),
        0..0,
        |_| None,
        Some(&dir),
    )
    .unwrap();
    assert_eq!(found.kind, Kind::Exact);
    assert!(!saved.exists());
}

/// Every symbol with a `.bytes` constant reads those bytes in the client
/// `ENSH_CLIENT_EXE` names; skipped without it.
#[test]
fn checked_symbols_read_their_bytes_in_the_client() {
    let Ok(path) = std::env::var("ENSH_CLIENT_EXE") else {
        return;
    };
    let bytes = std::fs::read(path).unwrap();
    let image = enshwick_gamedata::reflect::Image::parse(
        &bytes,
        enshwick_gamedata::reflect::Layout::File,
        None,
    )
    .unwrap();
    let tables = load();
    let client =
        tables::exact(&tables, tables::Side::Client, image.fingerprint().unwrap()).unwrap();
    let mut checked = 0;
    for symbol in client.symbols() {
        let resolved = client.resolve();
        if let Some(expected) = resolved.constant(&format!("{}.bytes", symbol.id())) {
            assert_eq!(
                image.u64(image.base() + symbol.rva()),
                Some(expected),
                "{}",
                symbol.id()
            );
            checked += 1;
        }
    }
    assert!(checked >= 42);
    let reflection = enshwick_gamedata::reflect::Reflection::scan(&image);
    for (ty, field, offset) in [
        ("keen::FbUiBundle", "namedStyles", 0x10a0c),
        ("keen::FbUiBundle", "sounds", 0x12060),
        ("keen::FbUiBundle", "locatags", 0x10),
        ("keen::FbUiBundle", "fontSizes", 0x109e0),
        ("keen::FbUiCharacterStyles", "menu", 0x1130),
        ("keen::FbUiCharacterStyles", "greenText", 0x210),
        ("keen::FbUiMenuCharacterStyles", "header1", 0x58),
        ("keen::FbUiSounds", "menu", 0x1e0),
        ("keen::FbUiSoundsMenuGeneric", "entryTrigger", 0xf0),
        ("keen::FbUiLocaLinks", "menuStrings", 0x54b0),
        ("keen::FbUiLocaMenus", "settings", 0x1c10),
        ("keen::FbUiLocaMenuSettings", "settingOff", 0xb0),
        ("keen::FbUiFontSizes", "caps_large", 0x24),
    ] {
        assert_eq!(
            reflection.field_offset(ty, field),
            Ok(offset),
            "{ty}.{field}"
        );
    }
}

/// On the server `ENSH_SERVER_EXE` names (skipped without it), finding
/// every entry by its pattern alone takes a small part of the 10 s the
/// stub holds the server's code for the loader.
#[test]
fn the_pattern_fallback_fits_the_server_hold() {
    let Ok(path) = std::env::var("ENSH_SERVER_EXE") else {
        return;
    };
    let bytes = std::fs::read(path).unwrap();
    let image = enshwick_gamedata::reflect::Image::parse(
        &bytes,
        enshwick_gamedata::reflect::Layout::File,
        None,
    )
    .unwrap();
    let text = image.section(".text").unwrap();
    let range = image.range(text);
    let code = image.read(range.start, range.end - range.start).unwrap();
    let tables = load();
    let started = std::time::Instant::now();
    let found = by_patterns(
        &tables,
        Side::SERVER,
        (code, range.start - image.base()),
        |rva| image.u32(image.base() + rva),
    )
    .unwrap();
    let took = started.elapsed();
    assert_eq!(found.kind, Kind::Fallback);
    // Unoptimised code scans about ten times slower than the release build.
    let budget = if cfg!(debug_assertions) { 15 } else { 2 };
    assert!(took < std::time::Duration::from_secs(budget), "{took:?}");
}
