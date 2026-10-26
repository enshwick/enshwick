//! A build table file, parsed, resolved and its patterns run over code.
//!
//! Fields: the table, the code section.

#![no_main]

use enshwick_fuzz::fields;
use enshwick_gamedata::Table;
use libfuzzer_sys::fuzz_target;

/// Where the fuzzed code section starts, as in the game's images.
const CODE_RVA: u64 = 0x1000;

fuzz_target!(|data: &[u8]| {
    let Some([text, code]) = fields::<2>(data) else {
        return;
    };
    let Ok(text) = core::str::from_utf8(text) else {
        return;
    };
    let Ok(table) = Table::parse(text) else {
        return;
    };
    drop(table.resolve());
    for symbol in table.symbols() {
        let _ = symbol.find(code, CODE_RVA);
    }
    for constant in table.constants() {
        let _ = table.find_pinned(constant, code, CODE_RVA);
        let _ = constant.find(code);
    }
});
