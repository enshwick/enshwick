//! Bounded proofs of the game memory reader's offset arithmetic. Each check
//! runs twice: Kani proves it for every input
//! (`cargo kani -p enshwick --tests`), and proptest runs it on random
//! inputs in every test run.

use proptest::prelude::*;

use super::{read_at, read_bytes_at, read_u32_atomic_at, write_at, write_bytes_at};

fn atomic_read(offset: u8, words: [u32; 3]) {
    let offset = usize::from(offset) % (size_of_val(&words) - 3);
    // SAFETY: `words` holds four readable bytes at every offset up to its
    // length less four, and no other thread writes it.
    let read = unsafe { read_u32_atomic_at(words.as_ptr().cast(), offset) };
    assert_eq!(read.is_some(), offset.is_multiple_of(4));
    if let Some(word) = read {
        assert_eq!(word, words[offset / 4]);
    }
}

fn span_read(offset: u8, len: u8, bytes: [u8; 16]) {
    let offset = usize::from(offset) % (bytes.len() + 1);
    let len = usize::from(len) % (bytes.len() - offset + 1);
    let mut out = vec![0u8; len];
    // SAFETY: `bytes` holds `len` readable bytes at `offset`, checked by the
    // two bounds above.
    unsafe { read_bytes_at(bytes.as_ptr(), offset, &mut out) };
    assert_eq!(out, bytes[offset..offset + len]);
}

fn value_round_trip(offset: u8, value: u64, bytes: [u8; 16]) {
    let offset = usize::from(offset) % (bytes.len() - 7);
    let mut buffer = bytes;
    // SAFETY: `buffer` holds eight writable bytes at `offset`, checked by
    // the bound above.
    unsafe { write_at(buffer.as_mut_ptr(), offset, value) };
    // SAFETY: the same eight bytes, now holding a u64.
    assert_eq!(unsafe { read_at::<u64>(buffer.as_ptr(), offset) }, value);
    assert_eq!(buffer[..offset], bytes[..offset]);
    assert_eq!(buffer[offset + 8..], bytes[offset + 8..]);
}

fn span_write(offset: u8, len: u8, bytes: [u8; 16], data: [u8; 16]) {
    let offset = usize::from(offset) % (bytes.len() + 1);
    let len = usize::from(len) % (bytes.len() - offset + 1);
    let mut buffer = bytes;
    // SAFETY: `buffer` holds `len` writable bytes at `offset`, checked by
    // the two bounds above.
    unsafe { write_bytes_at(buffer.as_mut_ptr(), offset, &data[..len]) };
    assert_eq!(buffer[offset..offset + len], data[..len]);
    assert_eq!(buffer[..offset], bytes[..offset]);
    assert_eq!(buffer[offset + len..], bytes[offset + len..]);
}

proptest! {
    #[test]
    fn an_atomic_read_takes_only_aligned_words(offset: u8, words: [u32; 3]) {
        atomic_read(offset, words);
    }

    #[test]
    fn a_span_reads_exactly_its_bytes(offset: u8, len: u8, bytes: [u8; 16]) {
        span_read(offset, len, bytes);
    }

    #[test]
    fn a_value_written_at_an_offset_reads_back_and_touches_nothing_else(
        offset: u8, value: u64, bytes: [u8; 16],
    ) {
        value_round_trip(offset, value, bytes);
    }

    #[test]
    fn a_span_writes_exactly_its_bytes(offset: u8, len: u8, bytes: [u8; 16], data: [u8; 16]) {
        span_write(offset, len, bytes, data);
    }
}

#[cfg(kani)]
#[kani::proof]
fn every_atomic_read_takes_only_aligned_words() {
    atomic_read(kani::any(), kani::any());
}

#[cfg(kani)]
#[kani::proof]
#[kani::unwind(18)]
fn every_span_reads_exactly_its_bytes() {
    span_read(kani::any(), kani::any(), kani::any());
}

#[cfg(kani)]
#[kani::proof]
#[kani::unwind(18)]
fn every_value_written_at_an_offset_reads_back_and_touches_nothing_else() {
    value_round_trip(kani::any(), kani::any(), kani::any());
}

#[cfg(kani)]
#[kani::proof]
#[kani::unwind(18)]
fn every_span_writes_exactly_its_bytes() {
    span_write(kani::any(), kani::any(), kani::any(), kani::any());
}
