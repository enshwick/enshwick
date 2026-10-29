//! Mod item data records the server sends a client: reading never panics,
//! and the records read write back to the bytes they came from.

#![no_main]

use enshwick_loader::item_records::{decode, encode};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let records = decode(data);
    let mut written = Vec::new();
    for &(id, pide, item, value) in &records {
        encode(&mut written, id, pide, item, value);
    }
    assert!(data.starts_with(&written));
    assert_eq!(decode(&written), records);
});
