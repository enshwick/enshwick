//! The game's archive read from damaged bytes: the header's maps, the
//! resources across their chunks and the contents in a container, all from
//! memory.
//!
//! Fields: the header, the resources, container 0.

#![no_main]

use enshwick_fuzz::fields;
use enshwick_gamedata::archive::Archive;
use libfuzzer_sys::fuzz_target;

/// Header words tried as type hashes and content hashes, at most.
const WORDS: usize = 64;
/// Resources read per input, at most.
const READS: usize = 32;

fuzz_target!(|data: &[u8]| {
    let Some([header, resources, container]) = fields::<3>(data) else {
        return;
    };
    let words: Vec<u32> = header
        .as_chunks::<4>()
        .0
        .iter()
        .take(WORDS)
        .map(|w| u32::from_le_bytes(*w))
        .collect();
    let Some(archive) = Archive::from_bytes(
        header.to_vec(),
        resources.to_vec(),
        [(0, container.to_vec())],
    ) else {
        return;
    };
    let mut read = 0;
    for &type_hash in &words {
        for key in archive.keys_of(type_hash) {
            if read == READS {
                break;
            }
            read += 1;
            drop(archive.resource(&key));
        }
    }
    for hash in words.windows(4) {
        if let Some(content) = archive.content(&[hash[0], hash[1], hash[2], hash[3]]) {
            assert_eq!(content.len() as u64, u64::from(hash[0]));
        }
    }
    archive.trim();
});
