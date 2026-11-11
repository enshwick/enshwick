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
    if named_executables().contains(&"ENSH_CLIENT_EXE") {
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

/// On the executables `ENSH_SERVER_EXE` and `ENSH_CLIENT_EXE` name (each
/// skipped without it), their own table with every routed row it pins taken
/// out, and the `.bytes` check of each symbol whose route checks its code,
/// gets each entry back from the engine routes, with the value the full
/// table pins; only those routes stand for a check, and nothing goes missing.
#[test]
fn a_table_without_its_routed_rows_gets_them_from_the_engine() {
    let routes = tables::Routes::embedded().unwrap();
    for (var, side, running) in [
        ("ENSH_SERVER_EXE", tables::Side::Server, Side::SERVER),
        ("ENSH_CLIENT_EXE", tables::Side::Client, Side::CLIENT),
    ] {
        let Ok(path) = std::env::var(var) else {
            continue;
        };
        let bytes = std::fs::read(path).unwrap();
        let image = enshwick_gamedata::reflect::Image::parse(
            &bytes,
            enshwick_gamedata::reflect::Layout::File,
            None,
        )
        .unwrap();
        let Some(full) = tables::exact(&load(), side, image.fingerprint().unwrap()).cloned() else {
            continue;
        };
        let text = tables::kept()
            .unwrap()
            .into_iter()
            .find(|(file, _)| *file == full.file_name())
            .unwrap()
            .1;
        let mut doc: toml_edit::DocumentMut = text.parse().unwrap();
        let pinned = full.resolve();
        let ids: Vec<&str> = routes
            .ids()
            .filter(|id| routes.routes(id, side))
            .filter(|id| pinned.symbol(id).or_else(|| pinned.constant(id)).is_some())
            .collect();
        for section in ["symbols", "constants"] {
            let rows = doc[section].as_table_mut().unwrap();
            for id in &ids {
                rows.remove(id);
                if routes.checks(id, side) {
                    rows.remove(&format!("{id}.bytes"));
                }
            }
        }
        let stripped = tables::Table::parse(&doc.to_string()).unwrap();
        let mut build = KnownBuild {
            name: stripped.name().to_owned(),
            table: stripped.resolve(),
            unanchored: Vec::new(),
            conflicts: Vec::new(),
            outside: Vec::new(),
            lacking: OnceLock::new(),
        };
        let reflection = enshwick_gamedata::reflect::Reflection::scan(&image);
        // Entries the full table does not pin come from the routes too; the
        // gamedata crate's route test checks each against the patterns.
        let found = build.route(&routes, &image, &reflection, running);
        let supplied = routes
            .ids()
            .filter(|id| routes.routes(id, side) && !ids.contains(id))
            .filter(|id| build.symbol(id).or_else(|| build.constant(id)).is_some())
            .count();
        assert_eq!(found, ids.len() + supplied, "{var}");
        for id in ids {
            assert_eq!(
                build.routed(id),
                routes.checks(id, side) || pinned.constant(id).is_some(),
                "{var}: {id}"
            );
            let (have, want) = match pinned.symbol(id) {
                Some(rva) => (build.symbol(id), Some(rva)),
                None => (build.constant(id), pinned.constant(id)),
            };
            assert_eq!(have, want, "{var}: {id}");
        }
        assert!(build.missing().is_empty(), "{var}: {:?}", build.missing());
    }
}

/// On the executables `ENSH_SERVER_EXE` and `ENSH_CLIENT_EXE` name (each
/// skipped without it), their own table without the join, the start of user
/// interactions, the division before the loop's waits and the workshop
/// limit's stop block, nor their `.bytes`, heads and bodies, gets each back
/// from its engine route with its code checked, and what each feature reads
/// in their place holds: the join is the provider's one slot, the division
/// leads to each wait in its function, and the stop block's length is the
/// table's body's.
#[cfg(windows)]
#[test]
#[expect(
    clippy::too_many_lines,
    reason = "one executable's four features, each a few lines over the same stripped build"
)]
fn routed_symbols_without_table_checks_serve_their_features() {
    use crate::pinned;
    let routes = tables::Routes::embedded().unwrap();
    let ids = [
        pinned::net::JOIN_GAME,
        pinned::user::START_INTERACTION,
        pinned::pacing::WAIT_DIVIDE,
        pinned::recipe::LIMIT_STOP,
    ];
    for (var, side, running) in [
        ("ENSH_SERVER_EXE", tables::Side::Server, Side::SERVER),
        ("ENSH_CLIENT_EXE", tables::Side::Client, Side::CLIENT),
    ] {
        let Ok(path) = std::env::var(var) else {
            continue;
        };
        let bytes = std::fs::read(path).unwrap();
        let image = enshwick_gamedata::reflect::Image::parse(
            &bytes,
            enshwick_gamedata::reflect::Layout::File,
            None,
        )
        .unwrap();
        let Some(full) = tables::exact(&load(), side, image.fingerprint().unwrap()).cloned() else {
            continue;
        };
        let text = tables::kept()
            .unwrap()
            .into_iter()
            .find(|(file, _)| *file == full.file_name())
            .unwrap()
            .1;
        let mut doc: toml_edit::DocumentMut = text.parse().unwrap();
        for id in ids {
            doc["symbols"].as_table_mut().unwrap().remove(id.id());
            doc["constants"]
                .as_table_mut()
                .unwrap()
                .remove(&format!("{id}.bytes"));
        }
        let stripped = tables::Table::parse(&doc.to_string()).unwrap();
        let mut build = KnownBuild {
            name: stripped.name().to_owned(),
            table: stripped.resolve(),
            unanchored: Vec::new(),
            conflicts: Vec::new(),
            outside: Vec::new(),
            lacking: OnceLock::new(),
        };
        let reflection = enshwick_gamedata::reflect::Reflection::scan(&image);
        build.route(&routes, &image, &reflection, running);
        let own = full.resolve();
        let base = image.base();
        let range = image.range(image.section(".text").unwrap());
        let code = range.start..range.end;
        for id in ids.into_iter().filter(|id| own.symbol(id.id()).is_some()) {
            assert_eq!(build.symbol(id.id()), own.symbol(id.id()), "{var}: {id}");
            assert!(
                build.routed(id.id()),
                "{var}: {id} is not checked by its route"
            );
            assert!(build.head(id.id()).is_none() && build.body(id.id()).is_none());
        }
        if let Some(join) = build.symbol(pinned::net::JOIN_GAME.id()) {
            let pins = crate::join::provider::Pins {
                routed: true,
                ..crate::join::provider::Pins::from_table(
                    |id| build.symbol(id),
                    |id| build.constant(id),
                    base,
                )
                .unwrap()
            };
            let read = |va: u64| image.u64(va);
            assert_eq!(
                crate::join::provider::verify(&pins, read, code.clone()),
                Ok(base + join),
                "{var}"
            );
        }
        if let (Some(stop), Some(next)) = (
            build.symbol(pinned::recipe::LIMIT_STOP.id()),
            build.symbol(pinned::recipe::LIMIT_NEXT.id()),
        ) {
            let block = image.read(base + stop, 64).unwrap();
            let len = crate::code::stop_len(block, base + stop, base + next);
            let body = own
                .body(pinned::recipe::LIMIT_STOP.id())
                .map(enshwick_gamedata::Body::len);
            assert!(len.is_some(), "{var}: the stop block has no end");
            assert_eq!(len, body.or(len), "{var}: the routed length is the table's");
        }
        if let Some(divide) = build.symbol(pinned::pacing::WAIT_DIVIDE.id()) {
            let pdata = image.range(image.section(".pdata").unwrap());
            let rows = image.read(pdata.start, pdata.end - pdata.start).unwrap();
            let end = rows
                .as_chunks::<12>()
                .0
                .iter()
                .find_map(|row| {
                    let begin = u64::from(u32::from_le_bytes(row[0..4].try_into().ok()?));
                    let end = u64::from(u32::from_le_bytes(row[4..8].try_into().ok()?));
                    (begin <= divide && divide < end).then_some(end)
                })
                .unwrap();
            let span = usize::try_from(end - divide).unwrap();
            for wait in [pinned::pacing::LOOP_WAIT, pinned::pacing::MESSAGE_WAIT] {
                if let Some(at) = build.symbol(wait.id()) {
                    assert!(
                        crate::pacing::after_division(divide, span, at),
                        "{var}: {wait}"
                    );
                    let between = image.read(base + divide, at - divide).unwrap();
                    assert!(
                        crate::pacing::keeps_left(between, base + divide),
                        "{var}: {wait} reads the time left"
                    );
                }
            }
        }
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
    if named_executables().contains(&"ENSH_CLIENT_EXE") {
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
    if named_executables().contains(&"ENSH_CLIENT_EXE") {
        assert!(clients.len() >= 2, "{rows:?}");
    }
    assert!(misreads(&rows).is_empty(), "{rows:?}");
    for (table, values) in clients.iter().filter(|(t, _)| t.ends_with(": own")) {
        // 0xb0 from client 26320373, where the slider grew 16 bytes.
        assert!(
            matches!(values[..], [Some(0xa0 | 0xb0)]),
            "{table}: {values:x?}"
        );
    }
}

/// The rows of `rows` where a carried table reads another value than its
/// executable's own table; a carried table that finds nothing is no misread.
fn misreads(rows: &[(String, Vec<Option<u64>>)]) -> Vec<&str> {
    rows.iter()
        .filter(|(table, values)| {
            let (var, _) = table.split_once(": ").unwrap_or_default();
            let own = rows.iter().find(|(t, _)| *t == format!("{var}: own"));
            own.is_some_and(|(_, own)| {
                values
                    .iter()
                    .zip(own)
                    .any(|(value, own)| value.is_some() && value != own)
            })
        })
        .map(|(table, _)| table.as_str())
        .collect()
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
    assert!(misreads(&rows).is_empty(), "{rows:?}");
    for (table, values) in rows.iter().filter(|(t, _)| t.ends_with(": own")) {
        assert_eq!(
            values,
            &[Some(0x20), Some(0x2c), Some(0x50), Some(0x28)],
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
    if named_executables().contains(&"ENSH_CLIENT_EXE") {
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
    let named = named_executables().len();
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
    let named = named_executables().len();
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
    let named = named_executables().len();
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
        "fog of war off on 1.0 client (26082781): map.fog_reveal does not read as its anchor says"
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

/// The anchors of the server carried to code where no pattern matches.
fn on_no_code() -> KnownBuild {
    let code = [0u8; 64];
    by_patterns(Side::SERVER, "server".into(), (&code, 0x1000), |_| None).unwrap()
}

#[cfg(windows)]
#[test]
fn a_build_names_the_loader_features_it_lacks_once() {
    if let Some(build) = kept(Side::SERVER) {
        assert_eq!(build.lacking(crate::TABLE_IDS), "");
    }
    let carried = on_no_code();
    let lacking = carried.lacking(crate::TABLE_IDS);
    assert!(lacking.split(", ").any(|name| name == "bases"), "{lacking}");
    assert_eq!(
        carried.lacking(&[]),
        lacking,
        "the first call names the parts"
    );
}

/// Any build runs on its anchors when given its code; with the setting off
/// only a build a compatibility report covers does.
#[test]
fn a_build_runs_on_its_anchors_when_allowed_and_given_code() {
    let unknown = Fingerprint::new([0xee; 16], 1);
    let code = [0u8; 64];
    let at = Some((&code[..], 0x1000));
    let found = select(Side::SERVER, unknown, "server".into(), at, |_| None).unwrap();
    assert_eq!(found.symbol("bases.holder_at"), None);
    assert!(found.missing().iter().any(|id| id == "bases.holder_at"));
    assert!(found.checks().is_empty());
    assert!(select(Side::SERVER, unknown, "server".into(), None, |_| None).is_none());
    assert!(select(Side::BOTH, unknown, "both".into(), at, |_| None).is_none());
    let tables = load();
    let Some(newest) = tables::newest(&tables, tables::Side::Server) else {
        return;
    };
    let reported = newest.fingerprint();
    configure(false);
    assert!(select(Side::SERVER, unknown, "server".into(), at, |_| None).is_none());
    assert!(select(Side::SERVER, reported, "server".into(), at, |_| None).is_some());
    configure(true);
}

/// A build is named by its side, Steam build, game version and executable,
/// each where it reads, and an absent entry says why: its anchor's pattern
/// found nothing, nothing finds it, the game's data did not give it, its
/// route and anchor disagree, or its route took a value outside its range.
#[test]
fn a_build_is_named_by_the_game_and_tells_why_an_entry_is_absent() {
    let fingerprint = Fingerprint::new([0xee; 16], 1);
    let executable = "executable eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee/1";
    assert_eq!(
        build_name(
            Side::SERVER,
            Some(SteamBuild::new(26_430_253)),
            Some("1287369|^/game38/branches/release_1.0|2026-11-03T10:00:00Z"),
            fingerprint
        ),
        format!("server, Steam build 26430253, game 1287369, {executable}")
    );
    assert_eq!(
        build_name(Side::CLIENT, None, None, fingerprint),
        format!("client, {executable}")
    );
    let mut found = on_no_code();
    let absent = Unhooked::Absent.to_string();
    assert_eq!(
        found.reason("bases.holder_at", &absent),
        "is not found by its anchor's pattern"
    );
    assert_eq!(
        found.reason("npc.nope", &absent),
        "is found by no engine route or anchor"
    );
    found.unfound("recipe.list_lookup");
    assert!(found.missing().iter().any(|id| id == "recipe.list_lookup"));
    assert_eq!(
        found.reason("recipe.list_lookup", &absent),
        "is found by no engine route or anchor"
    );
    assert_eq!(
        found.reason("attribute.health_max_slot", &absent),
        "is not read from the game's data"
    );
    found.conflict("chat.post");
    assert_eq!(
        found.reason("chat.post", &absent),
        "is found elsewhere by its engine route than by its anchor"
    );
    found.outside.push("ecs.add_components".to_owned());
    found.unfound("ecs.add_components");
    assert_eq!(
        found.reason("ecs.add_components", &absent),
        "is outside its range"
    );
    let moved = Unhooked::Moved.to_string();
    assert_eq!(found.reason("npc.lookup", &moved), moved);
}

/// A run on the anchors alone counts every entry only a route finds as
/// missing, so it never calls such a build complete.
#[test]
fn a_run_on_the_anchors_alone_misses_what_only_a_route_finds() {
    let routes = tables::Routes::embedded().unwrap();
    let mut found = on_no_code();
    found.unrouted(&routes, Side::SERVER);
    assert!(!found.complete());
    assert!(found.missing().iter().any(|id| id == "knowledge.push"));
    assert_eq!(
        found.reason("knowledge.push", &Unhooked::Absent.to_string()),
        "is found by no engine route or anchor"
    );
}

/// The Steam build is the appmanifest's `buildid`, beside a Steam library
/// (two folders up) or inside a `SteamCMD` install, also one at a drive's
/// root.
#[test]
fn the_steam_build_comes_from_the_appmanifest_where_steam_keeps_it() {
    let manifest =
        "\"AppState\"\n{\n\t\"appid\"\t\t\"2278520\"\n\t\"buildid\"\t\t\"26430253\"\n}\n";
    assert_eq!(manifest_build(manifest), Some(SteamBuild::new(26_430_253)));
    assert_eq!(manifest_build("\"buildid\"\t\"next\"\n"), None);
    let root = std::env::temp_dir().join(format!("enshwick-appmanifest-{}", std::process::id()));
    let library = root.join("steamapps");
    let client = library.join("common").join("Enshrouded");
    let server = root.join("server");
    std::fs::create_dir_all(&client).unwrap();
    std::fs::create_dir_all(server.join("steamapps")).unwrap();
    std::fs::write(library.join("appmanifest_1203620.acf"), manifest).unwrap();
    std::fs::write(
        server.join("steamapps").join("appmanifest_2278520.acf"),
        manifest,
    )
    .unwrap();
    // A one-component path has no library two folders up.
    let shallow = std::path::PathBuf::from(format!("enshwick-shallow-{}", std::process::id()));
    std::fs::create_dir_all(shallow.join("steamapps")).unwrap();
    std::fs::write(
        shallow.join("steamapps").join("appmanifest_2278520.acf"),
        manifest,
    )
    .unwrap();
    let found = (
        steam_build(&client, Side::CLIENT),
        steam_build(&server, Side::SERVER),
        steam_build(&server, Side::CLIENT),
        steam_build(&shallow, Side::SERVER),
    );
    std::fs::remove_dir_all(&root).unwrap();
    std::fs::remove_dir_all(&shallow).unwrap();
    let build = Some(SteamBuild::new(26_430_253));
    assert_eq!(found, (build, build, None, build));
}

/// No server table and no server anchor holds a row bases past 16 reads
/// only on a client, so a server never goes missing on an entry no server
/// feature reads.
#[test]
fn no_server_table_or_anchor_holds_a_client_only_bases_row() {
    use crate::pinned::bases;
    let client_only = [
        bases::BASE_IDS.id(),
        bases::CLIENT_CREATE.id(),
        bases::CLIENT_DESTROY.id(),
        bases::CLIENT_SIZE.id(),
        bases::RECORD_BASE_ID.id(),
    ];
    let anchors = tables::anchors(tables::Side::Server).unwrap();
    let servers = load()
        .into_iter()
        .filter(|t| t.side() == tables::Side::Server);
    for table in servers.chain([anchors]) {
        let resolved = table.resolve();
        for id in client_only {
            assert!(
                resolved.symbol(id).is_none() && resolved.constant(id).is_none(),
                "{}: {id}",
                table.name()
            );
        }
    }
}

/// A build logs at info when nothing is missing and warns with its error
/// code when entries are missing.
#[test]
fn a_build_warns_only_when_entries_are_missing() {
    use enshwick_core::abi::Level;
    if let Some(complete) = kept(Side::SERVER) {
        let (level, line) = complete.start_line();
        assert_eq!(level, Level::INFO);
        assert!(
            line.starts_with(&format!("game: {}, ", complete.name))
                && line.ends_with("entries found by engine routes and anchors"),
            "{line}"
        );
    }
    let (level, line) = on_no_code().start_line();
    assert_eq!(level, Level::WARN);
    assert!(
        line.contains(crate::errors::FEATURES_OFF.as_str()),
        "{line}"
    );
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
    let Some(client) = tables::exact(&tables, tables::Side::Client, image.fingerprint().unwrap())
    else {
        return;
    };
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
    // The offsets of client 26320373; other builds lay these types out
    // differently, so there each field only has to resolve.
    let exact = client.steam_build() == Some(SteamBuild::new(26_320_373));
    let moved = [
        ("keen::FbUiBundle", "namedStyles", 0x10bbc),
        ("keen::FbUiBundle", "sounds", 0x12210),
        ("keen::FbUiBundle", "locatags", 0x10),
        ("keen::FbUiBundle", "fontSizes", 0x10b90),
        ("keen::FbUiCharacterStyles", "menu", 0x1130),
        ("keen::FbUiCharacterStyles", "greenText", 0x210),
        ("keen::FbUiMenuCharacterStyles", "header1", 0x58),
        ("keen::FbUiSounds", "menu", 0x1e0),
        ("keen::FbUiSoundsMenuGeneric", "entryTrigger", 0xf0),
        ("keen::FbUiLocaLinks", "menuStrings", 0x5510),
        ("keen::FbUiLocaMenus", "settings", 0x1c40),
        ("keen::FbUiLocaMenuSettings", "settingOff", 0xb0),
        ("keen::FbUiFontSizes", "caps_large", 0x24),
    ]
    .into_iter()
    .filter_map(|(ty, field, offset)| {
        let found = reflection.field_offset(ty, field);
        let wrong = if exact {
            found != Ok(offset)
        } else {
            found.is_err()
        };
        wrong.then(|| format!("{ty}.{field}: {found:x?}, not {offset:#x}"))
    })
    .collect::<Vec<_>>();
    assert!(moved.is_empty(), "{moved:#?}");
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
    let started = std::time::Instant::now();
    let found = by_patterns(
        Side::SERVER,
        "server".into(),
        (code, range.start - image.base()),
        |rva| image.u32(image.base() + rva),
    );
    let took = started.elapsed();
    assert!(found.is_some());
    // Unoptimised code scans about ten times slower than the release build.
    let budget = if cfg!(debug_assertions) { 15 } else { 2 };
    assert!(took < std::time::Duration::from_secs(budget), "{took:?}");
}

/// On the server `ENSH_SERVER_EXE` names (skipped without it), another
/// program's jumps over the found functions hide them from a search of the
/// changed code, and the code as shipped, read in its place, finds every
/// entry again where it was.
#[test]
fn functions_hooked_before_the_search_are_found_in_the_code_as_shipped() {
    use enshwick_gamedata::reflect::{Image, Layout};

    let Ok(path) = std::env::var("ENSH_SERVER_EXE") else {
        return;
    };
    let shipped = std::fs::read(path).unwrap();
    let search = |image: &Image<'_>| {
        let range = image.range(image.section(".text").unwrap());
        let code = image.read(range.start, range.end - range.start).unwrap();
        by_patterns(
            Side::SERVER,
            "server".into(),
            (code, range.start - image.base()),
            |rva| image.u32(image.base() + rva),
        )
        .unwrap()
    };
    let image = Image::parse(&shipped, Layout::File, None).unwrap();
    let found = search(&image);
    let text = image.range(image.section(".text").unwrap());
    let mut hooked = shipped.clone();
    for &(_, rva) in found.symbols() {
        let Some(at) = image.read(image.base() + rva, 5) else {
            continue;
        };
        let offset = at.as_ptr().addr() - shipped.as_ptr().addr();
        hooked[offset..offset + 5].copy_from_slice(&[0xE9, 0x00, 0x10, 0x00, 0x00]);
    }
    let changed = Image::parse(&hooked, Layout::File, None).unwrap();
    let hidden = search(&changed);
    assert!(
        hidden.symbols().len() < found.symbols().len(),
        "{} of {} found in the hooked code",
        hidden.symbols().len(),
        found.symbols().len()
    );
    let code = image.read(text.start, text.end - text.start).unwrap();
    let again = search(&changed.with_text(code).unwrap());
    assert_eq!(again.symbols(), found.symbols());
    assert_eq!(again.constants(), found.constants());
}
