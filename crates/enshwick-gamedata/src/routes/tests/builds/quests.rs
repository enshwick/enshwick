//! The game's quest payment on every kept build: the function its quest
//! reward system calls to pay a quest's experience and set its bool.

/// Where `knowledge.quest_pay` starts, by table; `FUN_1401bbbb0` in the
/// decompiler on server 26430253 and its twin of the same code at
/// `0x41b7e0` on client 26430625 (2026-11-24), each the one call of
/// `sys.trigger_quest_rewards` to it.
pub(super) const QUEST_PAY_SYMBOLS: &[(&str, &str, u64)] = &[
    ("1.0 server (26430253)", "knowledge.quest_pay", 0x1b_bbb0),
    ("1.0 client (26430625)", "knowledge.quest_pay", 0x41_b7e0),
];
