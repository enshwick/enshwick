//! A server's query rules reply, from any server a player browses: the mod
//! list, names, pages and join values a game reads before it joins.

#![no_main]

use enshwick_loader::join::{
    mod_rules_value, mods_complete, name, page, plan, rule, server_values, sync_value, wanted,
};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|reply: &[u8]| {
    let mods = mod_rules_value(reply);
    let complete = mods_complete(reply);
    let ids: Vec<String> = wanted(&mods, "").into_iter().map(|(id, _)| id).collect();
    for id in ids.iter().take(16) {
        drop(name(reply, id));
        drop(page(reply, id));
        drop(sync_value(reply, id));
    }
    drop(server_values(reply, ids.iter().map(String::as_str)));
    drop(rule(reply, "enshwick"));
    let decided = plan("ours", "you.mod@1.0.0", Some(("theirs", &mods)), complete);
    let _ = decided.joins();
});
