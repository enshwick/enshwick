//! Bounded proofs that reading a value out of code bytes never panics and
//! stays inside the code. Each check runs twice: Kani proves it for every
//! input (`cargo kani -p enshwick-gamedata --tests`), and proptest runs it
//! on random inputs in every test run.

use proptest::prelude::*;

use super::{Operand, read};

/// Code bytes read at `at`, as many as `len` allows of `bytes`.
fn code_read(len: u8, at: u64, size: u8, bytes: [u8; 16]) {
    let code = &bytes[..usize::from(len) % (bytes.len() + 1)];
    let fits = size <= 8
        && usize::try_from(at)
            .ok()
            .and_then(|at| at.checked_add(usize::from(size)))
            .is_some_and(|end| end <= code.len());
    let value = read(code, at, size);
    assert_eq!(value.is_some(), fits);
    if let Some(value) = value {
        let at = usize::try_from(at).unwrap();
        let mut word = [0u8; 8];
        word[..usize::from(size)].copy_from_slice(&code[at..at + usize::from(size)]);
        assert_eq!(value, u64::from_le_bytes(word));
    }
}

/// An operand of any shape read out of any code at any start.
fn operand_read(shape: (u8, u8, u8, i64), start: u64, len: u8, bytes: [u8; 16]) {
    let (at, size, form, plus) = shape;
    let Some(mut operand) = Operand::new(u64::from(at), size) else {
        return;
    };
    operand = match form % 3 {
        0 => operand,
        1 => operand.negative(),
        _ => operand.shift(),
    };
    let operand = operand.plus(plus);
    let code = &bytes[..usize::from(len) % (bytes.len() + 1)];
    let _ = operand.read(code, start);
}

proptest! {
    #[test]
    fn a_read_out_of_code_is_its_little_endian_bytes_or_none(
        len: u8, at in 0u64..24, size in 0u8..10, bytes: [u8; 16],
    ) {
        code_read(len, at, size, bytes);
    }

    #[test]
    fn a_read_out_of_code_far_past_its_end_is_none(len: u8, at: u64, size: u8, bytes: [u8; 16]) {
        code_read(len, at, size, bytes);
    }

    #[test]
    fn an_operand_reads_any_code_without_panicking(
        at in 0u8..20, size: u8, form: u8, plus: i64, start: u64, len: u8, bytes: [u8; 16],
    ) {
        operand_read((at, size, form, plus), start, len, bytes);
        operand_read((at, size % 9, form, plus), start % 20, len, bytes);
    }
}

#[cfg(kani)]
#[kani::proof]
#[kani::unwind(17)]
fn every_read_out_of_code_is_its_little_endian_bytes_or_none() {
    code_read(kani::any(), kani::any(), kani::any(), kani::any());
}

#[cfg(kani)]
#[kani::proof]
#[kani::unwind(17)]
fn every_operand_reads_any_code_without_panicking() {
    operand_read(kani::any(), kani::any(), kani::any(), kani::any());
}
