//! The keys the client's modal windows let through on every kept client:
//! the game's rebuild of their list, its builder and setter, and the list.

/// The game's rebuild of the keys its modal windows let through, the
/// builder of an action's bound keys and the list's setter, by table; read
/// in the decompiler on client 26430625 (2026-11-24), the older clients' by
/// the same routes and checks.
pub(super) const PASS_SYMBOLS: &[(&str, &str, u64)] = &[
    ("1.0 client (26430625)", "ui.pass_refresh", 0xda_0270),
    ("1.0 client (26430625)", "ui.pass_bindings", 0xd2_0210),
    ("1.0 client (26430625)", "ui.pass_set", 0xa5_60b0),
    ("1.0 client (26320373)", "ui.pass_refresh", 0xda_01b0),
    ("1.0 client (26320373)", "ui.pass_bindings", 0xd2_0150),
    ("1.0 client (26320373)", "ui.pass_set", 0xa5_5ff0),
    ("1.0 client (26082781)", "ui.pass_refresh", 0xd8_aed0),
    ("1.0 client (26082781)", "ui.pass_bindings", 0xd0_d1e0),
    ("1.0 client (26082781)", "ui.pass_set", 0xa4_9660),
    ("1.0 client (1269579)", "ui.pass_refresh", 0xd8_9440),
    ("1.0 client (1269579)", "ui.pass_bindings", 0xd0_b750),
    ("1.0 client (1269579)", "ui.pass_set", 0xa4_7b30),
];

/// The list's holder off the interface context, its on byte, its keys and
/// their count, and a key's size, the same on every kept client
/// (`ui.window_end` and `ui.pass_set` in the decompiler on client 26430625,
/// 2026-11-24).
pub(super) const PASS_READS: [(&str, u64); 5] = [
    ("ui.pass_holder", 0x10),
    ("ui.pass_on", 0x930),
    ("ui.pass_keys", 0x688),
    ("ui.pass_count", 0x690),
    ("ui.pass_key_size", 0x28),
];
