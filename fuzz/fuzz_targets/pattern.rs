//! A code pattern from a build table or a mod's manifest, parsed and run
//! over code bytes.
//!
//! Fields: the pattern, the code.

#![no_main]

use enshwick::code::Pattern;
use enshwick_fuzz::fields;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Some([text, code]) = fields::<2>(data) else {
        return;
    };
    let Ok(text) = core::str::from_utf8(text) else {
        return;
    };
    let Ok(pattern) = Pattern::parse(text) else {
        return;
    };
    let found: Vec<usize> = pattern.find_all(code).take(64).collect();
    for &at in &found {
        assert!(pattern.matches(&code[at..]));
    }
    if let Ok(at) = pattern.find_unique(code) {
        assert_eq!(found, [at]);
    }
});
