//! A world's file of mod item data, as the server reads it on load: reading
//! never panics, and the records read write a file that reads back to them.

#![no_main]

use enshwick_loader::item_records::file::{read, write};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Some(records) = read(data) {
        let written = write(records.iter().cloned());
        assert_eq!(read(&written), Some(records));
    }
});
