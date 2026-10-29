//! Bounded proofs that the headed copies stay inside the size the other
//! side's header states. Each check runs twice: Kani proves it for every
//! input (`cargo kani -p enshwick --tests`), and proptest runs it on random
//! inputs in every test run.

use proptest::prelude::*;

use super::{Header, TickData, read_headed, write_headed};

/// Bytes past a whole [`TickData`] that a newer writer may state.
const NEWER: usize = 8;

/// `value`'s bytes; every [`TickData`] byte is a field, none is padding.
fn bytes_of(value: &TickData) -> &[u8] {
    // SAFETY: `value` is a live `TickData` of `size_of::<TickData>()`
    // initialised bytes: a header and `u64`s, no padding.
    unsafe { core::slice::from_raw_parts((&raw const *value).cast(), size_of::<TickData>()) }
}

/// `low` and then `high`, since proptest makes arrays of at most 32.
fn joined(low: [u8; 32], high: [u8; 32]) -> [u8; 64] {
    let mut bytes = [0; 64];
    bytes[..32].copy_from_slice(&low);
    bytes[32..].copy_from_slice(&high);
    bytes
}

/// The first `stated` bytes of `bytes`, with a header that states that
/// size.
fn written(bytes: &[u8], stated: usize) -> Vec<u8> {
    let mut buffer = bytes[..stated].to_vec();
    let size = u32::try_from(stated).unwrap();
    buffer[..4].copy_from_slice(&size.to_le_bytes());
    buffer
}

#[expect(
    clippy::cast_ptr_alignment,
    reason = "the headed copies take unaligned pointers"
)]
fn headed_read(stated: u8, required: u8, bytes: [u8; 64]) {
    let stated = usize::from(stated) % (size_of::<TickData>() + NEWER + 1);
    if stated < size_of::<Header>() {
        return;
    }
    let required = usize::from(required) % (size_of::<TickData>() + 1);
    let buffer = written(&bytes, stated);
    // SAFETY: `buffer` holds exactly the `stated` bytes its header states,
    // and nothing else touches it.
    let read = unsafe { read_headed(buffer.as_ptr().cast::<TickData>(), required) };
    assert_eq!(read.is_some(), required.max(size_of::<Header>()) <= stated);
    if let Some(tick) = read {
        let known = stated.min(size_of::<TickData>());
        let copied = bytes_of(&tick);
        assert_eq!(copied[..known], buffer[..known]);
        assert!(copied[known..].iter().all(|&b| b == 0));
    }
}

#[expect(
    clippy::cast_ptr_alignment,
    reason = "the headed copies take unaligned pointers"
)]
fn headed_write(stated: u8, step: u64, bytes: [u8; 64]) {
    let stated = usize::from(stated) % (size_of::<TickData>() + NEWER + 1);
    if stated < size_of::<Header>() {
        return;
    }
    let mut buffer = written(&bytes, stated);
    let before = buffer.clone();
    let value = TickData::new(step);
    // SAFETY: `buffer` holds exactly the `stated` bytes its header states,
    // and nothing else touches it.
    assert!(unsafe { write_headed(buffer.as_mut_ptr().cast::<TickData>(), value) });
    let known = stated.min(size_of::<TickData>());
    let size = u32::try_from(known).unwrap();
    assert_eq!(buffer[..4], size.to_le_bytes());
    assert_eq!(buffer[4..known], bytes_of(&value)[4..known]);
    assert_eq!(buffer[known..], before[known..]);
}

#[expect(
    clippy::cast_ptr_alignment,
    reason = "the headed copies take unaligned pointers"
)]
fn short_header_write(stated: u8, step: u64, bytes: [u8; 8]) {
    let stated = usize::from(stated) % size_of::<Header>();
    let mut buffer = bytes;
    buffer[..4].copy_from_slice(&u32::try_from(stated).unwrap().to_le_bytes());
    let before = buffer;
    let value = TickData::new(step);
    // SAFETY: `buffer` holds a whole header, more than it states.
    let wrote = unsafe { write_headed(buffer.as_mut_ptr().cast::<TickData>(), value) };
    assert!(!wrote);
    assert_eq!(buffer, before);
}

proptest! {
    #[test]
    fn a_headed_read_copies_only_what_the_header_states_and_zeroes_the_rest(
        stated: u8, required: u8, low: [u8; 32], high: [u8; 32],
    ) {
        headed_read(stated, required, joined(low, high));
    }

    #[test]
    fn a_headed_write_fills_only_the_room_the_header_states(
        stated: u8, step: u64, low: [u8; 32], high: [u8; 32],
    ) {
        headed_write(stated, step, joined(low, high));
    }

    #[test]
    fn a_header_shorter_than_a_header_takes_no_write(stated: u8, step: u64, bytes: [u8; 8]) {
        short_header_write(stated, step, bytes);
    }
}

#[cfg(kani)]
#[kani::proof]
#[kani::unwind(66)]
fn every_headed_read_copies_only_what_the_header_states() {
    headed_read(kani::any(), kani::any(), numbered());
}

#[cfg(kani)]
#[kani::proof]
#[kani::unwind(66)]
fn every_headed_write_fills_only_the_room_the_header_states() {
    headed_write(kani::any(), kani::any(), numbered());
}

/// Bytes that each name their own index, so a byte copied to or from the
/// wrong place shows. Symbolic bytes add nothing the lengths do not cover
/// and take the solver past the memory of a CI machine.
#[cfg(kani)]
fn numbered() -> [u8; 64] {
    core::array::from_fn(|i| u8::try_from(i).unwrap_or(u8::MAX))
}

#[cfg(kani)]
#[kani::proof]
#[kani::unwind(9)]
fn no_header_shorter_than_a_header_takes_a_write() {
    short_header_write(kani::any(), kani::any(), kani::any());
}
