//! The client's runtime key bindings on every kept client: where the
//! interface's state keeps them and how one action's binding is laid out.

/// Where the interface's state keeps the bindings and where their modifier
/// list starts, which move with the count of `GameplayAction` (139 on
/// 1269579 and 26082781, 140 after), by table; `0xe8b5a0` and the HUD's
/// `0xba2c40` in the decompiler on client 26430625 (2026-11-24), the table
/// seen live there (2026-11-23).
pub(super) const BINDING_SYMBOLS: &[(&str, &str, u64)] = &[
    ("1.0 client (26430625)", "input.bindings", 0x3_3cf0),
    ("1.0 client (26430625)", "input.binding_modifiers", 0x9a40),
    ("1.0 client (26320373)", "input.bindings", 0x3_3cf0),
    ("1.0 client (26320373)", "input.binding_modifiers", 0x9a40),
    ("1.0 client (26082781)", "input.bindings", 0x3_3cd0),
    ("1.0 client (26082781)", "input.binding_modifiers", 0x9928),
    ("1.0 client (1269579)", "input.bindings", 0x3_3cd0),
    ("1.0 client (1269579)", "input.binding_modifiers", 0x9928),
];

/// One action's binding: its size, its keyboard and mouse half's first and
/// second combination, a combination's modifier mask, a modifier's size in
/// the list and where the list keeps its items and count, the same on every
/// kept client (`0xe8b5a0` and
/// `0xeabd80` in the decompiler on client 26430625, 2026-11-24).
pub(super) const BINDING_READS: [(&str, u64); 7] = [
    ("input.binding_size", 0x118),
    ("input.binding_first", 0x28),
    ("input.binding_second", 0x5c),
    ("input.binding_mask", 0x2c),
    ("input.modifier_size", 0x28),
    ("input.modifier_items", 0),
    ("input.modifier_count", 8),
];
