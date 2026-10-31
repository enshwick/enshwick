//! Frame lines from another player's game, as the server and a client put
//! them back together, and the control messages they carry. A message this
//! side frames always comes back whole.
//!
//! Input: frame lines after their prefix, one per line.

#![no_main]

use enshwick_loader::wire::{
    MAX_MESSAGE, MAX_PARTS, PART_BYTES, PREFIX, Peer, assemble, frames, parse, read_hello,
    remote_message,
};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = core::str::from_utf8(data) else {
        return;
    };
    let mut peer = Peer::new();
    for line in text.lines() {
        let Some(frame) = parse(line) else {
            continue;
        };
        if let Some((_, message)) = assemble(&mut peer, frame) {
            assert!(message.len() <= MAX_PARTS * PART_BYTES);
            drop(read_hello(&message));
            drop(remote_message(&message));
        }
    }
    let message = &data[..data.len().min(MAX_MESSAGE)];
    let lines = frames(7, 1, message).unwrap_or_default();
    let mut whole = None;
    let mut sender = Peer::new();
    for line in lines.iter().rev() {
        let frame = line.strip_prefix(PREFIX).and_then(parse);
        whole = frame.and_then(|frame| assemble(&mut sender, frame));
    }
    assert_eq!(whole, Some((7, message.to_vec())));
});
