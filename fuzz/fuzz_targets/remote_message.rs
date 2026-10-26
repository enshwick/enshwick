//! A settings or actions message from another player: reading never panics,
//! and what reads writes back to the same bytes' message.

#![no_main]

use enshwick_loader::remote::Message;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Some((&kind, body)) = data.split_first() else {
        return;
    };
    if let Some(message) = Message::read(kind, body) {
        assert_eq!(message.kind(), kind);
        assert_eq!(Message::read(kind, &message.body()), Some(message));
    }
});
