//! A server's query rules reply, from any server a player browses: the mod
//! list a game reads before it joins, and the mismatch screen's rows.

#![no_main]

use enshwick_loader::base::join::{difference, mod_rules_value, mods_complete, plan, rule};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|reply: &[u8]| {
    let mods = mod_rules_value(reply);
    let complete = mods_complete(reply);
    drop(rule(reply, "enshwick"));
    let decided = plan("ours", "you.mod@1.0.0", Some(("theirs", &mods)), complete);
    let _ = decided.joins();
    let found = difference(&mods, "you.mod@1.0.0", "you.off@1.0.0", complete);
    assert!(
        found.missing.len() + found.mismatch.len() + found.extra.len()
            <= enshwick_loader::base::join::MAX_LINES
    );
});
