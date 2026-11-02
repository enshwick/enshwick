//! Packets from another player's game as the reliable link reads them, and
//! the packets it answers with: every message it hands on is within the
//! bound, and every packet it sends fits the packet size.
//!
//! Input: packets split at the byte `0xff`; the first byte sets how many
//! messages this side sends meanwhile.

#![no_main]

use std::time::{Duration, Instant};

use enshwick_loader::wire::{Link, MAX_SEALED};
use libfuzzer_sys::fuzz_target;

const PACKET: usize = 1200;

fuzz_target!(|data: &[u8]| {
    let Some(mut link) = Link::new(PACKET, 1) else {
        return;
    };
    let (sends, packets) = data.split_first().map_or((0, data), |(n, rest)| (*n, rest));
    for size in 0..sends {
        let _queued = link.send(&vec![size; usize::from(size) * 40]);
    }
    let start = Instant::now();
    for (step, packet) in (0u32..).zip(packets.split(|&b| b == 0xff)) {
        let _read = link.receive(packet, |message| assert!(message.len() <= MAX_SEALED));
        let now = start + Duration::from_millis(50) * step;
        let _alive = link.poll(now, 16, |out| {
            assert!(out.len() <= PACKET);
            step % 3 != 0
        });
    }
});
