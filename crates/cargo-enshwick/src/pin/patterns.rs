//! The code patterns of a build table: what finds each entry again on a
//! build that has no table of its own.
//!
//! A symbol's pattern is the smallest window of instructions around it, with
//! the parts that move between builds masked, that is unique in the code. A
//! constant's pattern is such a window around an instruction that uses the
//! value, with the value's own bytes fixed, so a match means that code still
//! uses it.

use std::ops::Range;

use super::exe::Exe;
use super::signature::Masked;

/// Windows with fewer significant bytes are not trusted.
const MIN_SIGNIFICANT: usize = 16;
/// The longest pattern written.
const MAX_LEN: usize = 96;
/// Constants smaller than this are too common in code to tell apart.
const MIN_CONSTANT: u64 = 0x10;

/// A symbol's pattern and how far past the match's start the symbol lies.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SymbolPattern {
    /// The pattern, as `code::Pattern` parses it.
    pub(crate) text: String,
    /// The symbol's offset from the match's start.
    pub(crate) at: u64,
}

/// The pattern of the symbol at `rva`, when the code around it has a unique
/// window.
pub(crate) fn symbol(exe: &Exe<'_>, rva: u64) -> Option<SymbolPattern> {
    let (bounds, masked) = decode(exe, rva)?;
    let offset = usize::try_from(rva - bounds.start).ok()?;
    let target = masked.insn_at(offset)?;
    let entry = rva == bounds.start;
    let span = unique_window(exe, &masked, target, entry)?;
    let start = masked.insns.get(span.start)?.offset;
    Some(SymbolPattern {
        text: render(&masked, &span)?,
        at: u64::try_from(offset - start).ok()?,
    })
}

/// The pattern of a use of `value` in the code of the functions at `near`.
pub(crate) fn constant(exe: &Exe<'_>, value: u64, near: &[u64]) -> Option<String> {
    if !(MIN_CONSTANT..=u64::from(u32::MAX)).contains(&value) {
        return None;
    }
    near.iter().find_map(|&rva| {
        let (_, masked) = decode(exe, rva)?;
        masked
            .insns
            .iter()
            .enumerate()
            .filter(|(_, insn)| insn.uses_as_field(value))
            .find_map(|(i, _)| {
                let span = unique_window(exe, &masked, i, false)?;
                render(&masked, &span)
            })
    })
}

/// The decoded code around `rva`: its function, or the stretch up to the
/// padding when no function table entry covers it.
fn decode(exe: &Exe<'_>, rva: u64) -> Option<(Range<u64>, Masked)> {
    let (bounds, in_table) = exe.code_around(rva)?;
    let code = exe.code(bounds.start, bounds.end - bounds.start)?;
    let mut masked = Masked::decode(code, bounds.start, &exe.absolute_range());
    if !in_table {
        masked.cut_at_padding();
    }
    Some((bounds, masked))
}

/// The smallest window of instructions around `target` that is unique in the
/// executable's code; only forward from it when `forward_only`.
fn unique_window(
    exe: &Exe<'_>,
    masked: &Masked,
    target: usize,
    forward_only: bool,
) -> Option<Range<usize>> {
    let (text, _) = exe.text();
    let count = masked.insns.len();
    let mut span = target..(target + 1).min(count);
    loop {
        let pattern = masked.window(span.clone())?;
        if pattern.bytes.len() > MAX_LEN {
            return None;
        }
        if pattern.significant() >= MIN_SIGNIFICANT && pattern.find(text, 2).len() == 1 {
            return Some(span);
        }
        let can_back = !forward_only && span.start > 0;
        let can_on = span.end < count;
        if !can_back && !can_on {
            return None;
        }
        if can_on && (!can_back || span.end - target <= target - span.start) {
            span.end += 1;
        } else {
            span.start -= 1;
        }
    }
}

/// The window as `code::Pattern` spells it: hex bytes, `??` for masked ones.
fn render(masked: &Masked, span: &Range<usize>) -> Option<String> {
    let pattern = masked.window(span.clone())?;
    let tokens: Vec<String> = pattern
        .bytes
        .iter()
        .zip(pattern.mask)
        .map(|(byte, &fixed)| {
            if fixed {
                format!("{byte:02X}")
            } else {
                "??".to_owned()
            }
        })
        .collect();
    Some(tokens.join(" "))
}
