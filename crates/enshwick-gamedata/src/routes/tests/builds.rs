//! The routes and the anchors on the kept game executables, each test
//! skipped without the variables that name them.

use super::super::{Routed, Routes};
use super::empty;
use crate::reflect::{Image, Layout, Reflection};
use crate::{Side, Table, exact, kept};

/// Entries a route finds on a table build that its own table does not pin
/// and no pattern of the newest table finds there either, by table name.
const UNPINNED_UNCHECKED: &[(&str, &[&str])] = &[
    (
        "1.0 client (1269579)",
        &[
            "bases.cursor",
            "bases.entity_query",
            "bases.set_insert",
            "building.block_box",
            "building.box_extent",
            "building.box_min",
            "building.box_size",
            "recipe.limit_stop",
            "ui.no_construct",
            "ui.scroll_focus",
            "ui.scroll_state_size",
            "ui.text_field",
            "ui.text_field_extent",
            "ui.text_field_text",
        ],
    ),
    (
        "1.0 server (1269579)",
        &[
            "bases.cursor",
            "bases.entity_query",
            "bases.set_insert",
            "building.block_box",
            "building.box_extent",
            "building.box_min",
            "building.box_size",
            "recipe.limit_stop",
        ],
    ),
];

/// Entries no table pins, found only by their route, with the value the
/// route must find on each table build, by table name.
const ROUTED_ONLY: &[(&str, &str, u64)] = &[
    (
        "1.0 client (1269579)",
        "ui.text_field_state_key",
        0x64d2_7908_60d9_c7f6,
    ),
    ("1.0 client (1269579)", "text.format", 0xae_f3d0),
    // The resource database's key listing, which the server lists its
    // templates through at start; on 26083076 checked live with a debugger.
    ("1.0 server (1269579)", "textures.keys", 0x81_eac0),
    ("1.0 server (1269579)", "textures.key_size", 0x20),
    ("1.0 server (26083076)", "textures.keys", 0x82_0e00),
    ("1.0 server (26083076)", "textures.key_size", 0x20),
    ("1.0 server (26319703)", "textures.keys", 0x82_a8a0),
    ("1.0 server (26319703)", "textures.key_size", 0x20),
    ("1.0 server (26430253)", "textures.keys", 0x82_a8a0),
    ("1.0 server (26430253)", "textures.key_size", 0x20),
    ("1.0 client (1269579)", "ui.dropdown_external", 0xc76),
    (
        "1.0 client (26082781)",
        "ui.text_field_state_key",
        0x64d2_7908_60d9_c7f6,
    ),
    (
        "1.0 client (26320373)",
        "ui.text_field_state_key",
        0x397c_0808_484b_9ef3,
    ),
    (
        "1.0 client (26430625)",
        "ui.text_field_state_key",
        0x7261_5e88_968a_8b89,
    ),
    ("1.0 client (26430625)", "text.format", 0xaf_f9f0),
    ("1.0 client (26430625)", "ui.dropdown_external", 0xc76),
    ("1.0 client (26082781)", "text.format", 0xaf_0f20),
    ("1.0 client (26320373)", "text.format", 0xaf_f930),
    ("1.0 client (26082781)", "ui.dropdown_external", 0xc76),
    ("1.0 client (26320373)", "ui.dropdown_external", 0xc76),
    // The text field's look, which its constructor zeroes and the rename
    // dialog sets; read in both on 26082781 and 26430625.
    ("1.0 client (1269579)", "ui.text_field_look", 0x148),
    ("1.0 client (26082781)", "ui.text_field_look", 0x148),
    ("1.0 client (26320373)", "ui.text_field_look", 0x148),
    ("1.0 client (26430625)", "ui.text_field_look", 0x148),
    // The row frame, the column frame's twin, and the separator line, at
    // their calls in and after the popup; read on 26430625.
    ("1.0 client (1269579)", "ui.row", 0xa2_eab0),
    ("1.0 client (1269579)", "ui.separator", 0xc8_1280),
    ("1.0 client (26082781)", "ui.row", 0xa3_05e0),
    ("1.0 client (26082781)", "ui.separator", 0xc8_2d10),
    ("1.0 client (26320373)", "ui.row", 0xa3_cf00),
    ("1.0 client (26320373)", "ui.separator", 0xc9_58a0),
    ("1.0 client (26430625)", "ui.row", 0xa3_cfc0),
    ("1.0 client (26430625)", "ui.separator", 0xc9_5960),
    // Where the separator reads its config's margin, height and offset,
    // which the loader's config must match; read on 26430625.
    ("1.0 client (1269579)", "ui.separator_margin", 0xc),
    ("1.0 client (1269579)", "ui.separator_height", 0x1c),
    ("1.0 client (1269579)", "ui.separator_offset", 0x20),
    ("1.0 client (26082781)", "ui.separator_margin", 0xc),
    ("1.0 client (26082781)", "ui.separator_height", 0x1c),
    ("1.0 client (26082781)", "ui.separator_offset", 0x20),
    ("1.0 client (26320373)", "ui.separator_margin", 0xc),
    ("1.0 client (26320373)", "ui.separator_height", 0x1c),
    ("1.0 client (26320373)", "ui.separator_offset", 0x20),
    ("1.0 client (26430625)", "ui.separator_margin", 0xc),
    ("1.0 client (26430625)", "ui.separator_height", 0x1c),
    ("1.0 client (26430625)", "ui.separator_offset", 0x20),
    // The button's icon flag and texture, read in `ui.button` and in the
    // vanilla gear buttons that set them on 26430625.
    ("1.0 client (1269579)", "ui.button_icon_shown", 0x48),
    ("1.0 client (1269579)", "ui.button_icon", 0x38),
    ("1.0 client (26082781)", "ui.button_icon_shown", 0x48),
    ("1.0 client (26082781)", "ui.button_icon", 0x38),
    ("1.0 client (26320373)", "ui.button_icon_shown", 0x48),
    ("1.0 client (26320373)", "ui.button_icon", 0x38),
    ("1.0 client (26430625)", "ui.button_icon_shown", 0x48),
    ("1.0 client (26430625)", "ui.button_icon", 0x38),
    // The button's kind, the first word of its config, which the frame
    // `ui.button` hands it to switches on; read on 26430625.
    ("1.0 client (1269579)", "ui.button_kind", 0),
    ("1.0 client (26082781)", "ui.button_kind", 0),
    ("1.0 client (26320373)", "ui.button_kind", 0),
    ("1.0 client (26430625)", "ui.button_kind", 0),
    // The console control handler the game hands `SetConsoleCtrlHandler`,
    // which starts its clean stop; read on 26430253.
    ("1.0 server (26083076)", "server.console_handler", 0x57_5fb0),
    ("1.0 server (1269579)", "server.console_handler", 0x57_5100),
    ("1.0 server (26319703)", "server.console_handler", 0x57_ba80),
    ("1.0 server (26430253)", "server.console_handler", 0x57_ba80),
    // The application the handler stops and its exit reason, which the
    // handler spins on when set; read on 26430253.
    ("1.0 server (26430253)", "server.application", 0x14d_75f0),
    ("1.0 server (26430253)", "server.exit_reason", 0xaa0),
    // The editor's commit, which every terrain edit passes; on 26083076 and
    // 26430253 checked live on server copies.
    ("1.0 server (1269579)", "voxel.commit", 0x83_39d0),
    ("1.0 server (26083076)", "voxel.commit", 0x83_5e00),
    ("1.0 server (26319703)", "voxel.commit", 0x83_fa70),
    ("1.0 server (26430253)", "voxel.commit", 0x83_fa70),
    ("1.0 client (1269579)", "voxel.commit", 0x109_e290),
    ("1.0 client (26082781)", "voxel.commit", 0x10a_03d0),
    ("1.0 client (26320373)", "voxel.commit", 0x10b_bd80),
    ("1.0 client (26430625)", "voxel.commit", 0x10b_b030),
    // The page cache's load end and eviction; on 26430253 checked live on
    // a server copy, on the others byte for byte.
    ("1.0 server (1269579)", "voxel.page_loaded", 0x84_86e7),
    ("1.0 server (26083076)", "voxel.page_loaded", 0x84_ab17),
    ("1.0 server (26319703)", "voxel.page_loaded", 0x85_48d7),
    ("1.0 server (26430253)", "voxel.page_loaded", 0x85_48d7),
    ("1.0 server (1269579)", "voxel.page_dropped", 0x84_8b0f),
    ("1.0 server (26083076)", "voxel.page_dropped", 0x84_af3f),
    ("1.0 server (26319703)", "voxel.page_dropped", 0x85_4cff),
    ("1.0 server (26430253)", "voxel.page_dropped", 0x85_4cff),
    // The session's join message handler, the reply it sends a refused
    // machine and the per-peer slots its ban branch closes; read on
    // 26430253 and 26430625.
    ("1.0 server (1269579)", "session.join_game", 0x9f_7c20),
    ("1.0 server (26319703)", "session.join_game", 0xa0_3f50),
    ("1.0 server (26430253)", "session.join_game", 0xa0_3fc0),
    ("1.0 client (26320373)", "session.join_game", 0xa7_00f0),
    ("1.0 client (26430625)", "session.join_game", 0xa7_01b0),
    ("1.0 server (1269579)", "session.join_reply", 0x9f_e4c0),
    ("1.0 server (26319703)", "session.join_reply", 0xa0_a820),
    ("1.0 server (26430253)", "session.join_reply", 0xa0_a890),
    ("1.0 client (26320373)", "session.join_reply", 0xa7_82e0),
    ("1.0 client (26430625)", "session.join_reply", 0xa7_83a0),
    ("1.0 server (1269579)", "session.peer_slots", 0x1_1418),
    ("1.0 server (26319703)", "session.peer_slots", 0x1_1418),
    ("1.0 server (26430253)", "session.peer_slots", 0x1_1418),
    ("1.0 client (26320373)", "session.peer_slots", 0x1_1988),
    ("1.0 client (26430625)", "session.peer_slots", 0x1_1988),
    ("1.0 server (1269579)", "session.peer_slot_size", 0x10),
    ("1.0 server (26319703)", "session.peer_slot_size", 0x10),
    ("1.0 server (26430253)", "session.peer_slot_size", 0x10),
    ("1.0 client (26320373)", "session.peer_slot_size", 0x10),
    ("1.0 client (26430625)", "session.peer_slot_size", 0x10),
    ("1.0 server (1269579)", "session.peer_slot_open", 0x1_141a),
    ("1.0 server (26319703)", "session.peer_slot_open", 0x1_141a),
    ("1.0 server (26430253)", "session.peer_slot_open", 0x1_141a),
    ("1.0 client (26320373)", "session.peer_slot_open", 0x1_198a),
    ("1.0 client (26430625)", "session.peer_slot_open", 0x1_198a),
    ("1.0 server (1269579)", "session.peer_slot_close", 0x1_1420),
    ("1.0 server (26319703)", "session.peer_slot_close", 0x1_1420),
    ("1.0 server (26430253)", "session.peer_slot_close", 0x1_1420),
    ("1.0 client (26320373)", "session.peer_slot_close", 0x1_1990),
    ("1.0 client (26430625)", "session.peer_slot_close", 0x1_1990),
    ("1.0 server (1269579)", "session.message_size", 0x490),
    ("1.0 server (26319703)", "session.message_size", 0x490),
    ("1.0 server (26430253)", "session.message_size", 0x490),
    ("1.0 client (26320373)", "session.message_size", 0x490),
    ("1.0 client (26430625)", "session.message_size", 0x490),
    ("1.0 server (1269579)", "session.reply_frame", 0x250),
    ("1.0 server (26319703)", "session.reply_frame", 0x250),
    ("1.0 server (26430253)", "session.reply_frame", 0x250),
    ("1.0 client (26320373)", "session.reply_frame", 0x250),
    ("1.0 client (26430625)", "session.reply_frame", 0x250),
    ("1.0 server (1269579)", "session.reply_banned_a", 0x2ac),
    ("1.0 server (26319703)", "session.reply_banned_a", 0x2ac),
    ("1.0 server (26430253)", "session.reply_banned_a", 0x2ac),
    ("1.0 client (26320373)", "session.reply_banned_a", 0x2ac),
    ("1.0 client (26430625)", "session.reply_banned_a", 0x2ac),
    ("1.0 server (1269579)", "session.reply_banned_b", 0x2a4),
    ("1.0 server (26319703)", "session.reply_banned_b", 0x2a4),
    ("1.0 server (26430253)", "session.reply_banned_b", 0x2a4),
    ("1.0 client (26320373)", "session.reply_banned_b", 0x2a4),
    ("1.0 client (26430625)", "session.reply_banned_b", 0x2a4),
    ("1.0 server (1269579)", "session.join_count", 0x38),
    ("1.0 server (26319703)", "session.join_count", 0x38),
    ("1.0 server (26430253)", "session.join_count", 0x38),
    ("1.0 client (26320373)", "session.join_count", 0x38),
    ("1.0 client (26430625)", "session.join_count", 0x38),
    ("1.0 server (1269579)", "session.join_accounts", 0x3c),
    ("1.0 server (26319703)", "session.join_accounts", 0x3c),
    ("1.0 server (26430253)", "session.join_accounts", 0x3c),
    ("1.0 client (26320373)", "session.join_accounts", 0x3c),
    ("1.0 client (26430625)", "session.join_accounts", 0x3c),
    ("1.0 server (1269579)", "session.join_account_kind", 0xc),
    ("1.0 server (26319703)", "session.join_account_kind", 0xc),
    ("1.0 server (26430253)", "session.join_account_kind", 0xc),
    ("1.0 client (26320373)", "session.join_account_kind", 0xc),
    ("1.0 client (26430625)", "session.join_account_kind", 0xc),
    ("1.0 server (1269579)", "session.join_account_id", 0x4),
    ("1.0 server (26319703)", "session.join_account_id", 0x4),
    ("1.0 server (26430253)", "session.join_account_id", 0x4),
    ("1.0 client (26320373)", "session.join_account_id", 0x4),
    ("1.0 client (26430625)", "session.join_account_id", 0x4),
    ("1.0 server (1269579)", "session.join_account_size", 0x10),
    ("1.0 server (26319703)", "session.join_account_size", 0x10),
    ("1.0 server (26430253)", "session.join_account_size", 0x10),
    ("1.0 client (26320373)", "session.join_account_size", 0x10),
    ("1.0 client (26430625)", "session.join_account_size", 0x10),
    ("1.0 server (1269579)", "session.platform", 0x80),
    ("1.0 server (26319703)", "session.platform", 0x80),
    ("1.0 server (26430253)", "session.platform", 0x80),
    ("1.0 client (26320373)", "session.platform", 0x80),
    ("1.0 client (26430625)", "session.platform", 0x80),
];

/// Entries no table pins, found only by their route, with the value the
/// route must find on every kept build of both sides.
const ROUTED_EVERYWHERE: &[(&str, u64)] = &[
    // The entity command buffer's AddComponents and RemoveComponents kinds
    // and the RemoveComponents layout; on server 26430253 checked live.
    ("ecs.add_components", 3),
    ("ecs.remove_components", 4),
    ("ecs.remove_components_hashes", 0xc),
    ("ecs.remove_components_size", 0x4c),
    // The first column grid's slot pointer, beside `voxel.grid_depth` at
    // 0x4c; on server 26430253 checked live.
    ("voxel.grid_slots", 0x58),
];

/// The interaction prompt's title, valid byte and verb in the interface's
/// play data, and the open screen's host entity, the same on every kept
/// client (static, research of 2026-11-06).
const PLAY_DATA_READS: [(&str, u64); 4] = [
    ("ui.prompt_title", 0x2bc0),
    ("ui.prompt_valid", 0x2bdd),
    ("ui.prompt_verb", 0x2bd8),
    ("ui.screen_host", 0x2c50),
];

/// The kept client tables, by name.
const CLIENT_TABLES: [&str; 4] = [
    "1.0 client (1269579)",
    "1.0 client (26082781)",
    "1.0 client (26320373)",
    "1.0 client (26430625)",
];

/// [`ROUTED_ONLY`] and the [`PLAY_DATA_READS`] of each of [`CLIENT_TABLES`].
fn routed_only() -> impl Iterator<Item = (&'static str, &'static str, u64)> {
    ROUTED_ONLY.iter().copied().chain(
        CLIENT_TABLES
            .into_iter()
            .flat_map(|table| PLAY_DATA_READS.map(|(id, value)| (table, id, value))),
    )
}

/// Entries a kept build's table pins that the loader needs on no build of
/// that side, by side: the server keeps the client's base set-up code but
/// never runs it (`anchors/classes.toml`).
const NOT_NEEDED: &[(Side, &[&str])] = &[(
    Side::Server,
    &[
        "bases.base_ids",
        "bases.client_create",
        "bases.client_destroy",
        "bases.client_size",
        "bases.record_base_id",
    ],
)];

/// The executables `ENSH_SERVER_EXE`, `ENSH_CLIENT_EXE` and the opt-in
/// `ENSH_EXES` (a path list, such as every kept build) name, each a server
/// when its file is `enshrouded_server.exe`.
fn executables() -> Vec<(std::path::PathBuf, Side)> {
    let mut paths: Vec<(std::path::PathBuf, Side)> = [
        ("ENSH_SERVER_EXE", Side::Server),
        ("ENSH_CLIENT_EXE", Side::Client),
    ]
    .into_iter()
    .filter_map(|(var, side)| Some((std::env::var_os(var)?.into(), side)))
    .collect();
    if let Some(list) = std::env::var_os("ENSH_EXES") {
        for path in std::env::split_paths(&list) {
            let server = path
                .file_name()
                .is_some_and(|name| name.eq_ignore_ascii_case("enshrouded_server.exe"));
            paths.push((path, if server { Side::Server } else { Side::Client }));
        }
    }
    paths
}

fn tables() -> Vec<Table> {
    kept()
        .unwrap()
        .iter()
        .map(|(_, text)| Table::parse(text).unwrap())
        .collect()
}

/// On every [`executables`] entry, every route of its side finds, with no
/// table behind it, the value its own table pins; an entry the table does
/// not pin takes the value [`ROUTED_ONLY`] or [`ROUTED_EVERYWHERE`] gives,
/// else the value the newest table's patterns find there, or is one of
/// [`UNPINNED_UNCHECKED`].
#[test]
#[cfg_attr(miri, ignore = "reading a game executable takes hours under Miri")]
fn every_route_holds_on_every_kept_build() {
    let tables = tables();
    let routes = Routes::embedded().unwrap();
    for (path, side) in executables() {
        let bytes = std::fs::read(&path).unwrap();
        let image = Image::parse(&bytes, Layout::File, None).unwrap();
        let reflection = Reflection::scan(&image);
        let Some(own) = exact(&tables, side, image.fingerprint().unwrap()) else {
            continue;
        };
        let own = own.resolve();
        let text = image.range(image.section(".text").unwrap());
        let code = image.read(text.start, text.end - text.start).unwrap();
        let carried = crate::newest(&tables, side).unwrap().fall_back(
            code,
            text.start - image.base(),
            |at| image.u32(image.base() + at),
        );
        let unchecked: &[&str] = UNPINNED_UNCHECKED
            .iter()
            .find(|(name, _)| *name == own.name())
            .map_or(&[], |(_, ids)| ids);
        let wrong: Vec<String> = routes
            .find(&image, &reflection, side, &empty())
            .into_iter()
            .filter_map(|(id, routed)| {
                let (value, constant) = match routed {
                    Some(Routed::Symbol(v) | Routed::Reached(v)) => (Some(v), false),
                    Some(Routed::Constant(v)) => (Some(v), true),
                    Some(Routed::Outside(_)) | None => (None, false),
                };
                let entry = |table: &crate::Resolved| {
                    if constant {
                        table.constant(id)
                    } else {
                        table.symbol(id)
                    }
                };
                let pinned = own.symbol(id).is_some() || own.constant(id).is_some();
                let routed_only = routed_only()
                    .find(|&(name, routed, _)| name == own.name() && routed == id)
                    .map(|(_, _, known)| known)
                    .or_else(|| {
                        ROUTED_EVERYWHERE
                            .iter()
                            .find(|&&(routed, _)| routed == id)
                            .map(|&(_, known)| known)
                    });
                let want = if pinned {
                    entry(&own)
                } else if let Some(known) = routed_only {
                    Some(known)
                } else if value.is_none() || unchecked.contains(&id) {
                    return None;
                } else {
                    entry(&carried)
                };
                (value != want).then(|| format!("{id}: route {routed:x?}, table {want:x?}"))
            })
            .collect();
        assert!(
            wrong.is_empty(),
            "{} ({}), {} wrong:\n{}",
            path.display(),
            own.name(),
            wrong.len(),
            wrong.join("\n")
        );
    }
}

/// On every [`executables`] entry, every constant route of its side finds
/// a value inside its `range` and `align`; a route without a range, or one
/// the values leave, fails with the row the kept values propose, for a
/// person to review.
#[test]
#[cfg_attr(miri, ignore = "reading a game executable takes hours under Miri")]
fn every_kept_build_s_constant_lies_in_its_route_s_range() {
    let routes = Routes::embedded().unwrap();
    let mut seen = std::collections::BTreeMap::<&str, (u64, u64)>::default();
    let mut outside = Vec::new();
    for (path, side) in executables() {
        let bytes = std::fs::read(&path).unwrap();
        let image = Image::parse(&bytes, Layout::File, None).unwrap();
        let reflection = Reflection::scan(&image);
        for (id, routed) in routes.find(&image, &reflection, side, &empty()) {
            let value = match routed {
                Some(Routed::Constant(v)) => v,
                Some(Routed::Outside(v)) => {
                    outside.push(format!("{id}: {v:#x} on {}", path.display()));
                    v
                }
                _ => continue,
            };
            let (lo, hi) = seen.entry(id).or_insert((value, value));
            *lo = (*lo).min(value);
            *hi = (*hi).max(value);
        }
    }
    let unbounded: Vec<String> = routes
        .routes
        .iter()
        .filter(|r| r.is_entry() && r.finds_constant() && r.range.is_none())
        .filter_map(|r| {
            let &(lo, hi) = seen.get(r.id.as_str())?;
            Some(match super::super::proposed_range(lo, hi) {
                (0, u64::MAX) => format!("{}: unbounded = true", r.id),
                (lo, hi) => format!("{}: range = [{lo:#x}, {hi:#x}]", r.id),
            })
        })
        .collect();
    assert!(
        outside.is_empty() && unbounded.is_empty(),
        "outside their range:\n{}\nwithout one, as the kept values propose:\n{}",
        outside.join("\n"),
        unbounded.join("\n")
    );
}

/// A [`ROUTED_ONLY`] or [`ROUTED_EVERYWHERE`] row with a table or route id
/// that does not exist would check nothing, so each names a kept table and a
/// route of its side, or a route of every side.
#[test]
fn every_routed_only_row_names_a_kept_table_and_a_route() {
    let tables = tables();
    if tables.is_empty() {
        return;
    }
    let routes = Routes::embedded().unwrap();
    for (name, id, _) in routed_only() {
        let table = tables.iter().find(|t| t.name() == name);
        assert!(
            table.is_some_and(|t| routes.routes(id, t.side())),
            "{name}: {id}"
        );
    }
    for &(id, _) in ROUTED_EVERYWHERE {
        for table in &tables {
            assert!(routes.routes(id, table.side()), "{}: {id}", table.name());
        }
    }
}

/// On every [`executables`] entry, the anchors of its side carried by their
/// patterns and then every engine route, with no table behind them, give
/// each entry its own table pins the value it pins, and miss, as the loader
/// counts a miss, nothing that table pins and on each side's newest build
/// nothing at all, but the attribute slots; the attribute slots,
/// which the loader reads from the archive, the `.bytes` of a symbol whose
/// route checks its code and the [`NOT_NEEDED`] entries are left out.
#[test]
#[cfg_attr(miri, ignore = "reading a game executable takes hours under Miri")]
fn the_anchors_and_routes_find_every_pinned_entry_without_a_table() {
    let tables = tables();
    let routes = Routes::embedded().unwrap();
    for (path, side) in executables() {
        let bytes = std::fs::read(&path).unwrap();
        let image = Image::parse(&bytes, Layout::File, None).unwrap();
        let reflection = Reflection::scan(&image);
        let Some(own) = exact(&tables, side, image.fingerprint().unwrap()) else {
            continue;
        };
        let text = image.range(image.section(".text").unwrap());
        let code = image.read(text.start, text.end - text.start).unwrap();
        let base = image.base();
        let mut found = crate::anchors(side)
            .unwrap()
            .fall_back(code, text.start - base, |at| image.u32(base + at));
        for (id, routed) in routes.find(&image, &reflection, side, &found.clone()) {
            if found.route(id, routed) == crate::Routing::Unresolved {
                found.lack(id);
            }
        }
        let pinned = own.resolve();
        let newest = crate::newest(&tables, side).is_some_and(|n| n.name() == own.name());
        let unslotted: Vec<&String> = found
            .missing()
            .iter()
            .filter(|id| found.attributes().iter().all(|(slot, _)| slot != *id))
            .filter(|id| newest || pinned.symbol(id).is_some() || pinned.constant(id).is_some())
            .collect();
        assert!(
            unslotted.is_empty(),
            "{} misses, as the loader would: {unslotted:?}",
            path.display()
        );
        let not_needed: &[&str] = NOT_NEEDED
            .iter()
            .find(|(s, _)| *s == side)
            .map_or(&[], |(_, ids)| ids);
        let wrong: Vec<String> = pinned
            .symbols()
            .iter()
            .map(|(id, rva)| (id, *rva, found.symbol(id)))
            .chain(
                pinned
                    .constants()
                    .iter()
                    .filter(|(id, _)| pinned.attributes().iter().all(|(a, _)| a != id))
                    .filter(|(id, _)| id.strip_suffix(".bytes").is_none_or(|s| !found.routed(s)))
                    .map(|(id, value)| (id, *value, found.constant(id))),
            )
            .filter(|(id, want, have)| *have != Some(*want) && !not_needed.contains(&id.as_str()))
            .map(|(id, want, have)| format!("{id}: found {have:x?}, table {want:#x}"))
            .collect();
        assert!(
            wrong.is_empty(),
            "{} ({}), {} wrong:\n{}",
            path.display(),
            own.name(),
            wrong.len(),
            wrong.join("\n")
        );
    }
}
