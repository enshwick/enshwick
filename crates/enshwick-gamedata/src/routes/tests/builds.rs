//! The routes and the anchors on the kept game executables, each test
//! skipped without the variables that name them.

use super::super::{Routed, Routes};
use super::empty;
use crate::reflect::{Image, Layout, Reflection};
use crate::{Side, Table, exact, kept};

mod attributes;
mod bindings;

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
    // The crafting menus' question whether a recipe shows, which their book
    // and the recipe tiles ask; read in the decompiler on 26430625, found by
    // the route on the other kept clients.
    ("1.0 client (26430625)", "recipe.menu_known", 0xd6_63b0),
    ("1.0 client (26320373)", "recipe.menu_known", 0xd6_62f0),
    ("1.0 client (1269579)", "recipe.menu_known", 0xd5_0c90),
    ("1.0 client (26082781)", "recipe.menu_known", 0xd5_2710),
    // The online provider's state tree transition, which its leave, its join
    // and every online error end in, found through the leave, which names no
    // state; read in the decompiler on 26430625, found by the route on the
    // other kept clients.
    (
        "1.0 client (26430625)",
        "net.provider_transition",
        0x10a_4c20,
    ),
    (
        "1.0 client (26320373)",
        "net.provider_transition",
        0x10a_5970,
    ),
    (
        "1.0 client (1269579)",
        "net.provider_transition",
        0x108_7c20,
    ),
    (
        "1.0 client (26082781)",
        "net.provider_transition",
        0x108_a100,
    ),
    // The provider's state with no session, which its leave (the
    // provider's function after its join) passes to that transition; read
    // in the decompiler on 26430625, found by the route on the other kept
    // clients.
    ("1.0 client (26430625)", "net.no_session_state", 1),
    ("1.0 client (26320373)", "net.no_session_state", 1),
    ("1.0 client (1269579)", "net.no_session_state", 1),
    ("1.0 client (26082781)", "net.no_session_state", 1),
    // The provider's state with a session, which its join passes to the
    // same transition; read in the decompiler on 26430625, found by the
    // route on the other kept clients.
    ("1.0 client (26430625)", "net.session_state", 2),
    ("1.0 client (26320373)", "net.session_state", 2),
    ("1.0 client (1269579)", "net.session_state", 2),
    ("1.0 client (26082781)", "net.session_state", 2),
    // The predicate the client tick shows the cursor by in the world, from
    // the HUD screen byte; read in the decompiler on 26430625, found by the
    // route on the other kept clients.
    (
        "1.0 client (26430625)",
        "ui.screen_shows_cursor",
        0x122_19f0,
    ),
    (
        "1.0 client (26320373)",
        "ui.screen_shows_cursor",
        0x122_26c0,
    ),
    ("1.0 client (1269579)", "ui.screen_shows_cursor", 0x120_3360),
    (
        "1.0 client (26082781)",
        "ui.screen_shows_cursor",
        0x120_5610,
    ),
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
    // The height in the box a node was laid out at (left, top, width,
    // height from 0x114), which `ui.frame_close` and `ui.button` read; read
    // on 26430625.
    ("1.0 client (1269579)", "ui.node_laid_height", 0x120),
    ("1.0 client (26082781)", "ui.node_laid_height", 0x120),
    ("1.0 client (26320373)", "ui.node_laid_height", 0x120),
    ("1.0 client (26430625)", "ui.node_laid_height", 0x120),
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
    ("1.0 server (1269579)", "server.application", 0x14c_85e0),
    ("1.0 server (1269579)", "server.exit_reason", 0xaa0),
    ("1.0 server (26083076)", "server.application", 0x14c_a5e0),
    ("1.0 server (26083076)", "server.exit_reason", 0xaa0),
    ("1.0 server (26319703)", "server.application", 0x14d_75f0),
    ("1.0 server (26319703)", "server.exit_reason", 0xaa0),
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
    ("1.0 server (1269579)", "session.find_machine", 0x9f_4860),
    ("1.0 server (26319703)", "session.find_machine", 0xa0_0b60),
    ("1.0 server (26430253)", "session.find_machine", 0xa0_0bd0),
    ("1.0 client (26320373)", "session.find_machine", 0xa6_c930),
    ("1.0 client (26430625)", "session.find_machine", 0xa6_c9f0),
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
    // The fall system's branch past its death record, read in the
    // decompiler on server 26430253 (2026-11-16); the client's is at the
    // same place in the same code.
    ("1.0 server (26430253)", "damage.fall_death", 0x8_a7bc),
    ("1.0 client (26430625)", "damage.fall_death", 0x27_8acc),
    // The same branch, found by its route, on the older kept builds.
    ("1.0 server (1269579)", "damage.fall_death", 0x8_5e9c),
    ("1.0 server (26083076)", "damage.fall_death", 0x8_6d4c),
    ("1.0 server (26319703)", "damage.fall_death", 0x8_a7bc),
    ("1.0 client (1269579)", "damage.fall_death", 0x27_446c),
    ("1.0 client (26082781)", "damage.fall_death", 0x27_460c),
    ("1.0 client (26320373)", "damage.fall_death", 0x27_89ec),
    // The same join path on the older kept builds.
    ("1.0 client (1269579)", "session.find_machine", 0xa5_e0c0),
    ("1.0 client (1269579)", "session.join_account_id", 0x4),
    ("1.0 client (1269579)", "session.join_account_kind", 0xc),
    ("1.0 client (1269579)", "session.join_account_size", 0x10),
    ("1.0 client (1269579)", "session.join_accounts", 0x3c),
    ("1.0 client (1269579)", "session.join_count", 0x38),
    ("1.0 client (1269579)", "session.join_game", 0xa6_1840),
    ("1.0 client (1269579)", "session.join_reply", 0xa6_9960),
    ("1.0 client (1269579)", "session.message_size", 0x490),
    ("1.0 client (1269579)", "session.peer_slot_close", 0x1_1990),
    ("1.0 client (1269579)", "session.peer_slot_open", 0x1_198a),
    ("1.0 client (1269579)", "session.peer_slot_size", 0x10),
    ("1.0 client (1269579)", "session.peer_slots", 0x1_1988),
    ("1.0 client (1269579)", "session.platform", 0x80),
    ("1.0 client (1269579)", "session.reply_banned_a", 0x2ac),
    ("1.0 client (1269579)", "session.reply_banned_b", 0x2a4),
    ("1.0 client (1269579)", "session.reply_frame", 0x250),
    ("1.0 client (26082781)", "session.find_machine", 0xa5_fc00),
    ("1.0 client (26082781)", "session.join_account_id", 0x4),
    ("1.0 client (26082781)", "session.join_account_kind", 0xc),
    ("1.0 client (26082781)", "session.join_account_size", 0x10),
    ("1.0 client (26082781)", "session.join_accounts", 0x3c),
    ("1.0 client (26082781)", "session.join_count", 0x38),
    ("1.0 client (26082781)", "session.join_game", 0xa6_3380),
    ("1.0 client (26082781)", "session.join_reply", 0xa6_b4a0),
    ("1.0 client (26082781)", "session.message_size", 0x490),
    ("1.0 client (26082781)", "session.peer_slot_close", 0x1_1990),
    ("1.0 client (26082781)", "session.peer_slot_open", 0x1_198a),
    ("1.0 client (26082781)", "session.peer_slot_size", 0x10),
    ("1.0 client (26082781)", "session.peer_slots", 0x1_1988),
    ("1.0 client (26082781)", "session.platform", 0x80),
    ("1.0 client (26082781)", "session.reply_banned_a", 0x2ac),
    ("1.0 client (26082781)", "session.reply_banned_b", 0x2a4),
    ("1.0 client (26082781)", "session.reply_frame", 0x250),
    ("1.0 server (26083076)", "session.find_machine", 0x9f_62a0),
    ("1.0 server (26083076)", "session.join_account_id", 0x4),
    ("1.0 server (26083076)", "session.join_account_kind", 0xc),
    ("1.0 server (26083076)", "session.join_account_size", 0x10),
    ("1.0 server (26083076)", "session.join_accounts", 0x3c),
    ("1.0 server (26083076)", "session.join_count", 0x38),
    ("1.0 server (26083076)", "session.join_game", 0x9f_9660),
    ("1.0 server (26083076)", "session.join_reply", 0x9f_ff00),
    ("1.0 server (26083076)", "session.message_size", 0x490),
    ("1.0 server (26083076)", "session.peer_slot_close", 0x1_1420),
    ("1.0 server (26083076)", "session.peer_slot_open", 0x1_141a),
    ("1.0 server (26083076)", "session.peer_slot_size", 0x10),
    ("1.0 server (26083076)", "session.peer_slots", 0x1_1418),
    ("1.0 server (26083076)", "session.platform", 0x80),
    ("1.0 server (26083076)", "session.reply_banned_a", 0x2ac),
    ("1.0 server (26083076)", "session.reply_banned_b", 0x2a4),
    ("1.0 server (26083076)", "session.reply_frame", 0x250),
    // `player_ui`'s local player check reads the id at +4 of its iterator
    // word; read in the decompiler, its live check is in-game.
    ("1.0 client (26430625)", "ecs.local_player_id", 4),
    // `player_ui`'s iterator, 0x3c8 bytes at its frame's +0x70, and the
    // word at +0x190 it hands its local player check; read in the
    // disassembly, live in the same check as `ecs.local_player_id`.
    (
        "1.0 client (26430625)",
        "ecs.player_ui_iterator_size",
        0x3c8,
    ),
    ("1.0 client (26430625)", "ecs.player_ui_iterator", 0x70),
    ("1.0 client (26430625)", "ecs.player_ui_local_player", 0x190),
    ("1.0 client (26320373)", "ecs.local_player_id", 4),
    (
        "1.0 client (26320373)",
        "ecs.player_ui_iterator_size",
        0x3c8,
    ),
    ("1.0 client (26320373)", "ecs.player_ui_iterator", 0x70),
    ("1.0 client (26320373)", "ecs.player_ui_local_player", 0x190),
    // One word shorter on 1269579, so the loader leaves the held read off
    // there.
    ("1.0 client (1269579)", "ecs.local_player_id", 4),
    ("1.0 client (1269579)", "ecs.player_ui_iterator_size", 0x3c0),
    ("1.0 client (1269579)", "ecs.player_ui_iterator", 0x70),
    ("1.0 client (1269579)", "ecs.player_ui_local_player", 0x190),
    ("1.0 client (26082781)", "ecs.local_player_id", 4),
    (
        "1.0 client (26082781)",
        "ecs.player_ui_iterator_size",
        0x3c0,
    ),
    ("1.0 client (26082781)", "ecs.player_ui_iterator", 0x70),
    ("1.0 client (26082781)", "ecs.player_ui_local_player", 0x190),
    // The client's input builder, `toggleHudScreen`, the recipe directory
    // column and the in-game menu's tab index; read in the decompiler on
    // 26430625, the older clients' by the same routes and checks.
    ("1.0 client (26430625)", "input.client_build", 0x24_77d0),
    ("1.0 client (26430625)", "ui.toggle_hud_screen", 0xd7_4850),
    ("1.0 client (26430625)", "ui.recipe_directory", 0xc7_5660),
    ("1.0 client (26430625)", "ui.menu_tab", 0x3_1e88),
    ("1.0 client (26320373)", "input.client_build", 0x24_76f0),
    ("1.0 client (26320373)", "ui.toggle_hud_screen", 0xd7_4790),
    ("1.0 client (26320373)", "ui.recipe_directory", 0xc7_55a0),
    ("1.0 client (26320373)", "ui.menu_tab", 0x3_1e88),
    ("1.0 client (26082781)", "input.client_build", 0x24_37c0),
    ("1.0 client (26082781)", "ui.toggle_hud_screen", 0xd6_0260),
    ("1.0 client (26082781)", "ui.recipe_directory", 0xc6_2a40),
    ("1.0 client (26082781)", "ui.menu_tab", 0x3_1e70),
    ("1.0 client (1269579)", "input.client_build", 0x24_3620),
    ("1.0 client (1269579)", "ui.toggle_hud_screen", 0xd5_e7e0),
    ("1.0 client (1269579)", "ui.recipe_directory", 0xc6_0ec0),
    ("1.0 client (1269579)", "ui.menu_tab", 0x3_1e70),
    // The input mapping's blocked byte, which the input builder checks
    // first; read in the decompiler on 26430625 (`mapping+0xad5a == 1`),
    // the older clients' by the same route.
    ("1.0 client (26430625)", "input.blocked", 0xad5a),
    ("1.0 client (26320373)", "input.blocked", 0xad5a),
    ("1.0 client (26082781)", "input.blocked", 0xac42),
    ("1.0 client (1269579)", "input.blocked", 0xac42),
    // The prompt's site in `player_ui`, after the title and the verb are
    // stored and before a lock may put its reason in the verb's place, and
    // the system's context in its home slot, which the site reads; read in
    // the decompiler on 26430625 (2026-11-16), the same bytes on 26320373;
    // the older clients' context by the same route, where the site compares
    // with an immediate the route does not match.
    ("1.0 client (26430625)", "ui.prompt_site", 0x2b_c3b6),
    ("1.0 client (26430625)", "ui.prompt_context", 0x2_d5e0),
    ("1.0 client (26320373)", "ui.prompt_site", 0x2b_c2d6),
    ("1.0 client (26320373)", "ui.prompt_context", 0x2_d5e0),
    ("1.0 client (26082781)", "ui.prompt_context", 0x2_d570),
    ("1.0 client (1269579)", "ui.prompt_context", 0x2_d570),
    // The put of one stack into one slot, which every player's move reaches;
    // read in the decompiler on server 26430253 and client 26430625, found
    // by the route, its start checked, on the other kept builds.
    ("1.0 server (26430253)", "inventory.put_slot", 0x1b_7100),
    ("1.0 server (26319703)", "inventory.put_slot", 0x1b_7100),
    ("1.0 server (26083076)", "inventory.put_slot", 0x1b_1900),
    ("1.0 server (1269579)", "inventory.put_slot", 0x1b_0a50),
    ("1.0 client (26430625)", "inventory.put_slot", 0x41_7170),
    ("1.0 client (26320373)", "inventory.put_slot", 0x41_7090),
    ("1.0 client (26082781)", "inventory.put_slot", 0x41_0200),
    ("1.0 client (1269579)", "inventory.put_slot", 0x40_fe80),
    // The global `ui.window_begin` reads its view from, the view's slot
    // that suffixes window names, and the name `ui.popup` opens its window
    // under; read in the decompiler on 26430625, the slot seen live there,
    // and on 1269579 and 26082781 (2026-11-17).
    ("1.0 client (26430625)", "ui.windows", 0x283_f408),
    ("1.0 client (26430625)", "ui.windows_view", 0x10),
    ("1.0 client (26430625)", "ui.view_slot", 0x44),
    ("1.0 client (26430625)", "ui.popup_window", 0x1d8_cf58),
    ("1.0 client (26430625)", "ui.popup_window_len", 0x12),
    ("1.0 client (26320373)", "ui.windows", 0x284_0408),
    ("1.0 client (26320373)", "ui.windows_view", 0x10),
    ("1.0 client (26320373)", "ui.view_slot", 0x44),
    ("1.0 client (26320373)", "ui.popup_window", 0x1d8_df58),
    ("1.0 client (26320373)", "ui.popup_window_len", 0x12),
    ("1.0 client (26082781)", "ui.windows", 0x281_e3f8),
    ("1.0 client (26082781)", "ui.windows_view", 0x10),
    ("1.0 client (26082781)", "ui.view_slot", 0x44),
    ("1.0 client (26082781)", "ui.popup_window", 0x1d6_d028),
    ("1.0 client (26082781)", "ui.popup_window_len", 0x12),
    ("1.0 client (1269579)", "ui.windows", 0x281_b3f8),
    ("1.0 client (1269579)", "ui.windows_view", 0x10),
    ("1.0 client (1269579)", "ui.view_slot", 0x44),
    ("1.0 client (1269579)", "ui.popup_window", 0x1d6_aeb8),
    ("1.0 client (1269579)", "ui.popup_window_len", 0x12),
    // The node map `ui.action_query` looks a bound node id up in, and the
    // context's field it reaches the map through; read in the decompiler
    // on 26430625, the same code on the other kept clients (2026-11-17).
    ("1.0 client (26430625)", "ui.node_find", 0x11a_f900),
    ("1.0 client (26430625)", "ui.context_nodes", 0x18),
    ("1.0 client (26320373)", "ui.node_find", 0x11b_05d0),
    ("1.0 client (26320373)", "ui.context_nodes", 0x18),
    ("1.0 client (26082781)", "ui.node_find", 0x119_4450),
    ("1.0 client (26082781)", "ui.context_nodes", 0x18),
    ("1.0 client (1269579)", "ui.node_find", 0x119_21c0),
    ("1.0 client (1269579)", "ui.context_nodes", 0x18),
];

/// The client control reads: the input builder's stick, look, target
/// selection and buttons, and the recipe browser's highlighted recipe and
/// select slot, read in the decompiler on client 26430625 (research of
/// 2026-11-14 and 2026-11-16: the builder clears these four while the game
/// blocks the input). On the other kept clients the routes found the same
/// values, which pins them there but does not prove the routes matched the
/// right code.
const CLIENT_CONTROL_READS: [(&str, u64); 6] = [
    ("input.movement_stick", 0x2b8),
    ("input.camera_rotation", 0x2c4),
    ("input.target_selection", 0x2ec),
    ("input.digital_input", 0x3a8),
    ("ui.recipe_select", 0x48),
    ("ui.recipe_highlighted", 0xe8),
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
    // Where `inventory.txn_init` copies the descriptor's owner index; read
    // in the decompiler on server 26430253 and client 26430625, found by the
    // route on every kept build.
    ("inventory.txn_owners", 0x90),
    // Where `inventory.put_slot` writes its "put nothing" result: the error
    // byte, the zero beside it and the count left over; read in the
    // decompiler on server 26430253, found by the route on every kept build.
    ("inventory.put_out_error", 0),
    ("inventory.put_out_placed", 4),
    ("inventory.put_out_left", 8),
];

/// The text field's kept byte that starts its typing at its next input, as
/// its navigation's confirm does; read in the decompiler on client 26430625
/// (2026-11-16), the same on every kept client.
const WIDGET_READS: [(&str, u64); 1] = [("ui.text_field_start", 0x198a)];

/// The interaction prompt's title, valid byte and verb in the interface's
/// play data, and the open screen's host entity, the same on every kept
/// client (static, research of 2026-11-06); the screen a stacked screen
/// returns to, beside the HUD screen byte (`set_screen`, client 26430625,
/// 2026-11-15).
const PLAY_DATA_READS: [(&str, u64); 5] = [
    ("ui.prompt_title", 0x2bc0),
    ("ui.prompt_valid", 0x2bdd),
    ("ui.prompt_verb", 0x2bd8),
    ("ui.screen_host", 0x2c50),
    ("ui.return_screen", 0x1cd9),
];

/// The game's item slot widget and its data's init, and the item viewer's
/// two sites after its last row, by table, read in the decompiler on client
/// 26430625 (2026-11-16, research notes item-tooltip-lines and
/// ui-native-rows); the older clients' by the same routes.
const ITEM_UI_SYMBOLS: &[(&str, &str, u64)] = &[
    ("1.0 client (26430625)", "ui.item_slot", 0xc0_46c0),
    ("1.0 client (26430625)", "ui.item_slot_init", 0xb1_f4e0),
    ("1.0 client (26430625)", "ui.tooltip_panel_site", 0xc1_1fa0),
    ("1.0 client (26430625)", "ui.tooltip_plain_site", 0xc1_1fb5),
    ("1.0 client (26320373)", "ui.item_slot", 0xc0_4600),
    ("1.0 client (26320373)", "ui.item_slot_init", 0xb1_f420),
    ("1.0 client (26320373)", "ui.tooltip_panel_site", 0xc1_1ee0),
    ("1.0 client (26320373)", "ui.tooltip_plain_site", 0xc1_1ef5),
    ("1.0 client (26082781)", "ui.item_slot", 0xbf_5070),
    ("1.0 client (26082781)", "ui.item_slot_init", 0xb0_fd50),
    ("1.0 client (26082781)", "ui.tooltip_panel_site", 0xc0_2880),
    ("1.0 client (26082781)", "ui.tooltip_plain_site", 0xc0_2895),
    ("1.0 client (1269579)", "ui.item_slot", 0xbf_34f0),
    ("1.0 client (1269579)", "ui.item_slot_init", 0xb0_e230),
    ("1.0 client (1269579)", "ui.tooltip_panel_site", 0xc0_0d00),
    ("1.0 client (1269579)", "ui.tooltip_plain_site", 0xc0_0d15),
];

/// The item slot's click flag, the local player's data and item table it reads, and
/// the item viewer's layout, unknown-item flag, item and item entity, the
/// same on every kept client (client 26430625 in the decompiler,
/// 2026-11-16). The slot widget's answer, its stack record and its data's
/// init extent: the state block a local at 0x60 holds, copied to the answer
/// at 0x10, its focus, pointer, press and hit bytes at 0x60, 0x62, 0x65 and
/// 0x69, the context's input record at 0x10 and its navigation byte at 1
/// (`ui.item_slot` reads the pointer only while that byte is 0), the
/// record's count at 0x10 and its 0x48 bytes, and the 0x91 bytes the init
/// writes (`doItemSlot` and its init in the decompiler on client 26430625,
/// 2026-11-16; the same on every kept client).
const ITEM_UI_READS: [(&str, u64); 20] = [
    ("ui.item_slot_clickable", 0x62),
    ("ui.item_slot_state", 0x10),
    ("ui.item_slot_state_local", 0x60),
    ("ui.item_slot_focused", 0x60),
    ("ui.item_slot_pointer", 0x62),
    ("ui.context_input", 0x10),
    ("ui.input_navigating", 0x1),
    ("ui.item_slot_pressed", 0x65),
    ("ui.item_slot_hit", 0x69),
    ("ui.item_slot_count", 0x10),
    ("ui.item_slot_record_size", 0x48),
    ("ui.item_slot_init_end", 0x91),
    ("ui.context_player", 0xd0),
    ("ui.player_data", 0x38),
    ("ui.player_items", 0x46_67b8),
    ("ui.viewer_layout", 0xbc),
    ("ui.viewer_unknown", 0xe3),
    ("ui.viewer_item", 0x8),
    ("ui.viewer_pide", 0xc),
    ("ui.attribute_row_size", 0x2a0),
];

/// The sound system's start, stop and instance calls a client mod's sounds
/// go through (`sound.rs` of the loader's ui layer; read in the decompiler
/// on client 26430625, 2026-11-23; on the older clients the routes find
/// one match each whose prologue holds its check).
const SOUND_SYMBOLS: &[(&str, &str, u64)] = &[
    ("1.0 client (26430625)", "sound.start", 0x106_ee50),
    ("1.0 client (26430625)", "sound.stop", 0x106_fc50),
    ("1.0 client (26430625)", "sound.set_volume", 0x106_9c10),
    ("1.0 client (26430625)", "sound.set_position", 0x106_9ba0),
    ("1.0 client (26430625)", "sound.is_playing", 0x105_e8e0),
    ("1.0 client (26320373)", "sound.start", 0x106_fba0),
    ("1.0 client (26320373)", "sound.stop", 0x107_09a0),
    ("1.0 client (26320373)", "sound.set_volume", 0x106_a960),
    ("1.0 client (26320373)", "sound.set_position", 0x106_a8f0),
    ("1.0 client (26320373)", "sound.is_playing", 0x105_eb30),
    ("1.0 client (26082781)", "sound.start", 0x105_46a0),
    ("1.0 client (26082781)", "sound.stop", 0x105_54a0),
    ("1.0 client (26082781)", "sound.set_volume", 0x104_f680),
    ("1.0 client (26082781)", "sound.set_position", 0x104_f610),
    ("1.0 client (26082781)", "sound.is_playing", 0x104_4460),
    ("1.0 client (1269579)", "sound.start", 0x105_1a40),
    ("1.0 client (1269579)", "sound.stop", 0x105_2840),
    ("1.0 client (1269579)", "sound.set_volume", 0x104_ca20),
    ("1.0 client (1269579)", "sound.set_position", 0x104_c9b0),
    ("1.0 client (1269579)", "sound.is_playing", 0x104_1800),
];

/// The reads that reach the sound system and its scenes from the
/// interface's context: the sound object at 0x10 of the context, its
/// system at 0x60 and `Ui` scene at 0x68; the client object's link at 0x730,
/// which the `GameUi` keeps at 0x58, and the `Ui` scene at 0x69c, which it
/// keeps at 0x78 (`0xa4f310`, the client set-up `0x8520a0` and the `GameUi`
/// constructor `0xa44070` in the decompiler on client 26430625,
/// 2026-11-23).
const SOUND_READS: [(&str, u64); 7] = [
    ("sound.ui_object", 0x10),
    ("sound.ui_system", 0x60),
    ("sound.ui_scene", 0x68),
    ("sound.client_link", 0x730),
    ("sound.client_ui_scene", 0x69c),
    ("sound.gameui_link", 0x58),
    ("sound.gameui_ui_scene", 0x78),
];

/// The kept client tables, by name.
const CLIENT_TABLES: [&str; 4] = [
    "1.0 client (1269579)",
    "1.0 client (26082781)",
    "1.0 client (26320373)",
    "1.0 client (26430625)",
];

/// [`ROUTED_ONLY`] and [`ITEM_UI_SYMBOLS`], and the [`PLAY_DATA_READS`],
/// [`CLIENT_CONTROL_READS`], [`WIDGET_READS`] and [`ITEM_UI_READS`] of each
/// of [`CLIENT_TABLES`].
fn routed_only() -> impl Iterator<Item = (&'static str, &'static str, u64)> {
    ROUTED_ONLY
        .iter()
        .chain(ITEM_UI_SYMBOLS)
        .chain(SOUND_SYMBOLS)
        .chain(bindings::BINDING_SYMBOLS)
        .chain(attributes::ATTRIBUTE_SYMBOLS)
        .copied()
        .chain(CLIENT_TABLES.into_iter().flat_map(|table| {
            PLAY_DATA_READS
                .into_iter()
                .chain(CLIENT_CONTROL_READS)
                .chain(WIDGET_READS)
                .chain(ITEM_UI_READS)
                .chain(SOUND_READS)
                .chain(bindings::BINDING_READS)
                .chain(attributes::ATTRIBUTE_ROW_READS)
                .chain(attributes::ATTRIBUTE_TEXT_READS)
                .map(move |(id, value)| (table, id, value))
        }))
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
