//! The Character screen's row builders on every kept client: their
//! addresses and the text each one opens its group with.

use rustc_hash::FxHashSet;

use crate::routes::shape::Step;
use crate::routes::{How, Kin, Routes};

/// The Character screen's row builders, one per group, and the push they
/// share, by table, read in the decompiler on client 26430625 (2026-11-16).
/// The older clients' come from the same routes, each of which finds the
/// one builder that reads the text reflection names for its group on that
/// client ([`ATTRIBUTE_TEXT_READS`]).
pub(super) const ATTRIBUTE_SYMBOLS: &[(&str, &str, u64)] = &[
    ("1.0 client (26430625)", "ui.attribute_row_push", 0xd6_bd60),
    ("1.0 client (26430625)", "ui.attribute_rows_main", 0xd1_c990),
    (
        "1.0 client (26430625)",
        "ui.attribute_rows_fog_survival",
        0xd2_3b90,
    ),
    (
        "1.0 client (26430625)",
        "ui.attribute_rows_primary",
        0xd2_06e0,
    ),
    (
        "1.0 client (26430625)",
        "ui.attribute_rows_regeneration",
        0xd2_1770,
    ),
    ("1.0 client (26430625)", "ui.attribute_rows_misc", 0xd1_e250),
    (
        "1.0 client (26430625)",
        "ui.attribute_rows_damage",
        0xd1_5890,
    ),
    (
        "1.0 client (26430625)",
        "ui.attribute_rows_weapon_damage",
        0xd2_5eb0,
    ),
    (
        "1.0 client (26430625)",
        "ui.attribute_rows_enemy_damage",
        0xd1_8420,
    ),
    (
        "1.0 client (26430625)",
        "ui.attribute_rows_protection",
        0xd2_2f90,
    ),
    ("1.0 client (26320373)", "ui.attribute_row_push", 0xd6_bca0),
    ("1.0 client (26320373)", "ui.attribute_rows_main", 0xd1_c8d0),
    (
        "1.0 client (26320373)",
        "ui.attribute_rows_fog_survival",
        0xd2_3ad0,
    ),
    (
        "1.0 client (26320373)",
        "ui.attribute_rows_primary",
        0xd2_0620,
    ),
    (
        "1.0 client (26320373)",
        "ui.attribute_rows_regeneration",
        0xd2_16b0,
    ),
    ("1.0 client (26320373)", "ui.attribute_rows_misc", 0xd1_e190),
    (
        "1.0 client (26320373)",
        "ui.attribute_rows_damage",
        0xd1_57d0,
    ),
    (
        "1.0 client (26320373)",
        "ui.attribute_rows_weapon_damage",
        0xd2_5df0,
    ),
    (
        "1.0 client (26320373)",
        "ui.attribute_rows_enemy_damage",
        0xd1_8360,
    ),
    (
        "1.0 client (26320373)",
        "ui.attribute_rows_protection",
        0xd2_2ed0,
    ),
    ("1.0 client (1269579)", "ui.attribute_row_push", 0xd5_6530),
    ("1.0 client (1269579)", "ui.attribute_rows_main", 0xd0_7f00),
    (
        "1.0 client (1269579)",
        "ui.attribute_rows_fog_survival",
        0xd0_efe0,
    ),
    (
        "1.0 client (1269579)",
        "ui.attribute_rows_primary",
        0xd0_bc20,
    ),
    (
        "1.0 client (1269579)",
        "ui.attribute_rows_regeneration",
        0xd0_cca0,
    ),
    ("1.0 client (1269579)", "ui.attribute_rows_misc", 0xd0_97c0),
    (
        "1.0 client (1269579)",
        "ui.attribute_rows_damage",
        0xd0_0940,
    ),
    (
        "1.0 client (1269579)",
        "ui.attribute_rows_weapon_damage",
        0xd1_1340,
    ),
    (
        "1.0 client (1269579)",
        "ui.attribute_rows_enemy_damage",
        0xd0_3960,
    ),
    (
        "1.0 client (1269579)",
        "ui.attribute_rows_protection",
        0xd0_e3e0,
    ),
    ("1.0 client (26082781)", "ui.attribute_row_push", 0xd5_7fb0),
    ("1.0 client (26082781)", "ui.attribute_rows_main", 0xd0_9990),
    (
        "1.0 client (26082781)",
        "ui.attribute_rows_fog_survival",
        0xd1_0a70,
    ),
    (
        "1.0 client (26082781)",
        "ui.attribute_rows_primary",
        0xd0_d6b0,
    ),
    (
        "1.0 client (26082781)",
        "ui.attribute_rows_regeneration",
        0xd0_e730,
    ),
    ("1.0 client (26082781)", "ui.attribute_rows_misc", 0xd0_b250),
    (
        "1.0 client (26082781)",
        "ui.attribute_rows_damage",
        0xd0_23d0,
    ),
    (
        "1.0 client (26082781)",
        "ui.attribute_rows_weapon_damage",
        0xd1_2dd0,
    ),
    (
        "1.0 client (26082781)",
        "ui.attribute_rows_enemy_damage",
        0xd0_53f0,
    ),
    (
        "1.0 client (26082781)",
        "ui.attribute_rows_protection",
        0xd0_fe70,
    ),
];

/// Where the game reads a Character screen row's and row list's fields, the
/// same on every kept client: through a row pointer in the row widget, the
/// value text and the push; the submenu's hovered row at 0x3a8, its tooltip
/// read at 0x3c0 and the byte 1 its empty row holds at 0x628; the pair flag
/// 4 past the second value; the grid cell's two words the widget compares
/// with -1 (`doAttributesSubmenu`, its row widget 0x140b8f0b0, value text
/// 0x140d486c0 and push 0x140d6bd60 in the decompiler on client 26430625,
/// 2026-11-16 and 2026-11-17).
pub(super) const ATTRIBUTE_ROW_READS: [(&str, u64); 15] = [
    ("ui.attribute_row_hovered", 0x3a8),
    ("ui.attribute_row_hovered_tooltip", 0x3c0),
    ("ui.attribute_row_hovered_flag", 0x628),
    ("ui.attribute_row_cell", 0x288),
    ("ui.attribute_row_cell_second", 0x290),
    ("ui.attribute_row_label_length", 8),
    ("ui.attribute_row_value", 0x30),
    ("ui.attribute_row_second", 0x34),
    ("ui.attribute_row_pair_after_second", 4),
    ("ui.attribute_row_trend", 0x3c),
    ("ui.attribute_row_seconds", 0x281),
    ("ui.attribute_row_milliseconds", 0x282),
    ("ui.attribute_row_percent", 0x298),
    ("ui.attribute_row_plus", 0x299),
    ("ui.attribute_list_rows", 0),
];

/// Per group of the Character screen, where reflection puts the text that
/// opens the group (`FbUiLocaAttributes`' health, fogResistance,
/// constitution, healthRegeneration, healing, offenseCritical, staffDamage,
/// foeMagicalDamage and blockAttribute under
/// `FbUiBundle.locatags.rpgTerms.attributes`), which the group's row
/// builder reads first: read in the decompiler on client 26430625
/// (2026-11-16) and the same on every kept client.
pub(super) const ATTRIBUTE_TEXT_READS: [(&str, u64); 9] = [
    ("ui.attribute_text_main", 0x3a0),
    ("ui.attribute_text_fog_survival", 0x460),
    ("ui.attribute_text_primary", 0x760),
    ("ui.attribute_text_regeneration", 0x820),
    ("ui.attribute_text_misc", 0x10e0),
    ("ui.attribute_text_damage", 0x12e0),
    ("ui.attribute_text_weapon_damage", 0xc20),
    ("ui.attribute_text_enemy_damage", 0xe60),
    ("ui.attribute_text_protection", 0x1420),
];

/// Each builder route finds the one callee of the submenu that reads the
/// text reflection names for its group, so a reordered submenu still links
/// each builder to its own group: every builder route names its own group's
/// text and no other, and the nine texts differ, so no two routes can find
/// one builder.
#[test]
fn every_builder_route_reads_its_own_group_s_text_and_the_nine_texts_differ() {
    let routes = Routes::embedded().unwrap();
    for &(text, _) in &ATTRIBUTE_TEXT_READS {
        let group = text.strip_prefix("ui.attribute_text_").unwrap();
        let builder = format!("ui.attribute_rows_{group}");
        let route = routes.routes.iter().find(|r| r.id == builder).unwrap();
        let How::Among {
            kin: Kin::Callees(_),
            steps,
        } = &route.how
        else {
            panic!("{builder} is no `callees` route");
        };
        let named: Vec<&str> = steps.iter().flat_map(Step::entries).collect();
        assert_eq!(named, [text], "{builder}");
    }
    let texts: FxHashSet<u64> = ATTRIBUTE_TEXT_READS.iter().map(|&(_, t)| t).collect();
    assert_eq!(texts.len(), ATTRIBUTE_TEXT_READS.len());
}

/// The push is found among the callees of every group's builder alike, so
/// no group's rows hang on another group's builder route; the Primary
/// builder pushes inline and gives nothing, on every kept client.
#[test]
fn the_push_route_looks_through_every_group_s_builder() {
    let routes = Routes::embedded().unwrap();
    let push = routes
        .routes
        .iter()
        .find(|r| r.id == "ui.attribute_row_push")
        .unwrap();
    let How::Among {
        kin: Kin::Callees(callers),
        ..
    } = &push.how
    else {
        panic!("the push is no `callees` route");
    };
    let mut builders: Vec<String> = ATTRIBUTE_TEXT_READS
        .iter()
        .map(|&(text, _)| text.replace("ui.attribute_text_", "ui.attribute_rows_"))
        .collect();
    let mut callers = callers.clone();
    builders.sort_unstable();
    callers.sort_unstable();
    assert_eq!(callers, builders);
}
