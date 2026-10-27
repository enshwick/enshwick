//! The mod lists a server advertises and a player's hello carries, as the
//! join plan and the admission gate read them.
//!
//! Fields: our fingerprint, our mods, the server's fingerprint, the server's
//! mods.

#![no_main]

use enshwick_fuzz::text_fields;
use enshwick_loader::admission::{Hello, needs};
use enshwick_loader::join::{Names, differences, plan, wanted};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Some([ours, ours_mods, server, server_mods]) = text_fields::<4>(data) else {
        return;
    };
    let complete = ours.len() % 2 == 0;
    for advertised in [Some((server, server_mods)), None] {
        let decided = plan(ours, ours_mods, advertised, complete);
        let _ = decided.joins();
        let _ = decided.turned_off();
    }
    drop(wanted(server_mods, ours_mods));
    drop(differences(server_mods, ours_mods, &Names::new()));
    let hello = Hello {
        mods: ours_mods.to_owned(),
        complete,
        nonce: 0,
        language: None,
    };
    drop(needs(server_mods, Some(&hello)));
    drop(needs(server_mods, None));
});
