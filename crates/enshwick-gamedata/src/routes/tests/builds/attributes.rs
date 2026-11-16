//! The Character screen's row builders on every kept client: their
//! addresses and the text each one opens its group with.

use rustc_hash::FxHashSet;

/// The Character screen's row builders, one per group, and the push they
/// share, by table, read in the decompiler on client 26430625 (2026-11-16).
/// The older clients' come from the same
/// routes and are confirmed by their own reflection, not by the routes: on
/// each, the builder at the address opens with the text reflection names
/// for its group ([`ATTRIBUTE_TEXT_READS`]).
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
/// value text and the push; the submenu's hovered row at 0x3a8 and its
/// tooltip read at 0x3c0; the pair flag 4 past the second value; the
/// breakdown flag a fog survival builder sets at 0x238 for the row 0x40
/// below its frame (`doAttributesSubmenu`, its row widget 0x140b8f0b0, value
/// text 0x140d486c0, push 0x140d6bd60 and builder 0x140d23b90 in the
/// decompiler on client 26430625, 2026-11-16).
pub(super) const ATTRIBUTE_ROW_READS: [(&str, u64); 15] = [
    ("ui.attribute_row_hovered", 0x3a8),
    ("ui.attribute_row_hovered_tooltip", 0x3c0),
    ("ui.attribute_row_label_length", 8),
    ("ui.attribute_row_value", 0x30),
    ("ui.attribute_row_second", 0x34),
    ("ui.attribute_row_pair_after_second", 4),
    ("ui.attribute_row_trend", 0x3c),
    ("ui.attribute_row_seconds", 0x281),
    ("ui.attribute_row_milliseconds", 0x282),
    ("ui.attribute_row_percent", 0x298),
    ("ui.attribute_row_plus", 0x299),
    ("ui.attribute_row_breakdown_set", 0x238),
    ("ui.attribute_row_breakdown_row", 0x40),
    ("ui.attribute_list_rows", 0),
    ("ui.attribute_list_filled", 8),
];

/// Per group of the Character screen, where its row builder first reads the
/// UI bundle's texts and where reflection puts the text that opens the
/// group (`FbUiLocaAttributes`' health, fogResistance, constitution,
/// healthRegeneration, healing, offenseCritical, staffDamage,
/// foeMagicalDamage and blockAttribute under
/// `FbUiBundle.locatags.rpgTerms.attributes`): read in the decompiler on
/// client 26430625 (2026-11-16) and the same on every kept client, where
/// reflection names each builder by the text it opens with.
pub(super) const ATTRIBUTE_TEXT_READS: [(&str, &str, u64); 9] = [
    (
        "ui.attribute_rows_main_text",
        "ui.attribute_text_main",
        0x3a0,
    ),
    (
        "ui.attribute_rows_fog_survival_text",
        "ui.attribute_text_fog_survival",
        0x460,
    ),
    (
        "ui.attribute_rows_primary_text",
        "ui.attribute_text_primary",
        0x760,
    ),
    (
        "ui.attribute_rows_regeneration_text",
        "ui.attribute_text_regeneration",
        0x820,
    ),
    (
        "ui.attribute_rows_misc_text",
        "ui.attribute_text_misc",
        0x10e0,
    ),
    (
        "ui.attribute_rows_damage_text",
        "ui.attribute_text_damage",
        0x12e0,
    ),
    (
        "ui.attribute_rows_weapon_damage_text",
        "ui.attribute_text_weapon_damage",
        0xc20,
    ),
    (
        "ui.attribute_rows_enemy_damage_text",
        "ui.attribute_text_enemy_damage",
        0xe60,
    ),
    (
        "ui.attribute_rows_protection_text",
        "ui.attribute_text_protection",
        0x1420,
    ),
];

/// A builder's check is the text it opens with: two groups that opened with
/// one text could not tell a reordered submenu apart, so the nine texts
/// differ, and each group's two routes are distinct entries.
#[test]
fn every_pair_of_the_nine_builders_opens_with_different_texts() {
    let texts: FxHashSet<u64> = ATTRIBUTE_TEXT_READS.iter().map(|&(.., t)| t).collect();
    assert_eq!(texts.len(), ATTRIBUTE_TEXT_READS.len());
    let ids: FxHashSet<&str> = ATTRIBUTE_TEXT_READS
        .iter()
        .flat_map(|&(read, named, _)| [read, named])
        .collect();
    assert_eq!(ids.len(), 2 * ATTRIBUTE_TEXT_READS.len());
}
