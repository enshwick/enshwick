//! Messages from another player's game on Enshwick's own socket, as both
//! sides open them, and the control messages they carry. A payload this
//! side seals always opens to the same channel and bytes.
//!
//! Input: one received message.

#![no_main]

use enshwick_loader::wire::{
    MAX_PAYLOAD, MAX_SEALED, is_executable, open, read_hello, remote_message, seal, sealed_size,
};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok((channel, payload)) = open(data) {
        assert!(payload.len() <= MAX_PAYLOAD && !is_executable(payload));
        if channel == 0 {
            drop(read_hello(payload));
            drop(remote_message(payload));
        }
    }
    let payload = &data[..data.len().min(MAX_PAYLOAD)];
    let channel = data
        .first_chunk::<4>()
        .map_or(0, |bytes| u32::from_le_bytes(*bytes));
    let mut out = [0; MAX_SEALED];
    match seal(&mut out, channel, payload) {
        Some(size) => {
            assert_eq!(size, sealed_size(channel, payload.len()));
            assert_eq!(open(&out[..size]), Ok((channel, payload)));
        }
        None => assert!(is_executable(payload)),
    }
});
