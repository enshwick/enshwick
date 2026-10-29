//! Notice messages the server sends a client, queued and raised as the
//! client's steps raise them: reading never panics, the queue stays within
//! its bounds and the text the game shows is a whole-character prefix of at
//! most its longest.
//!
//! Input: notice messages after their control kind, split at `0xff`.

#![no_main]

use enshwick_loader::notice_queue::{MAX_TEXT, MAX_WAITING, SLOTS, Waiting, cut, push, step_batch};
use enshwick_loader::wire::{NOTICE, control, notice, read_notice};
use libfuzzer_sys::fuzz_target;

/// Notices queued between two steps.
const PER_STEP: usize = 3;

fn show(waiting: &mut Waiting) {
    let batch = step_batch(waiting);
    assert!(batch.len() <= SLOTS);
    for (_, text) in &batch {
        let shown = cut(text);
        assert!(shown.len() <= MAX_TEXT);
        assert!(text.starts_with(shown));
        if text.len() <= MAX_TEXT {
            assert_eq!(shown, text);
        }
    }
}

fuzz_target!(|data: &[u8]| {
    let mut waiting = Waiting::new();
    let mut dropping = false;
    let messages = data.split(|&b| b == enshwick_fuzz::SEPARATOR);
    for (n, message) in messages.enumerate() {
        if let Some((entity, text)) = read_notice(message) {
            let sent = notice(entity, text);
            let rest = sent.strip_prefix(control(NOTICE).as_slice());
            assert_eq!(rest.and_then(read_notice), Some((entity, text)));
            let _pushed = push(&mut waiting, &mut dropping, entity, text);
            assert!(waiting.len() <= MAX_WAITING);
        }
        if n % PER_STEP == PER_STEP - 1 {
            show(&mut waiting);
        }
    }
    while !waiting.is_empty() {
        show(&mut waiting);
    }
});
