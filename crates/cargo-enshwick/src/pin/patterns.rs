//! The code patterns of a build table: what finds each entry again on a
//! build that has no table of its own.
//!
//! A pattern is the smallest window of instructions around the code it is
//! about that is unique in the code section, with the parts that move
//! between builds masked. Where the window stays unique with every field
//! and frame offset masked too, that looser form is taken, so a change of
//! layout elsewhere in the window does not break it.
//!
//! - A symbol's pattern covers its own code, or the instruction that calls
//!   or addresses it, whose rel32 then gives the symbol.
//! - A constant's pattern covers the instruction a human picked as the use
//!   of the value, with the operand that carries it masked and read on the
//!   other build, so a moved field gives its new offset. Without such an
//!   operand, as for an offset of 0, the pattern fixes the instruction and
//!   only confirms the value. No pattern is guessed for a constant: a value
//!   found in some code says nothing about what that code means.

use std::ops::Range;

use anyhow::{Context as _, bail};
use enshwick_gamedata::{Body, Locate};

use super::exe::Exe;
use super::signature::{Insn, Masked, Owned, Reference};

/// Windows with fewer significant bytes are not trusted.
const MIN_SIGNIFICANT: usize = 16;
/// The longest pattern written, the longest a table reads.
const MAX_LEN: usize = enshwick_gamedata::MAX_PATTERN;

/// A symbol's pattern and where the symbol lies from its match.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SymbolPattern {
    /// The pattern, as `code::Pattern` parses it.
    pub(crate) text: String,
    /// Where the symbol lies from the match's start.
    pub(crate) locate: Locate,
}

/// Which operand of an instruction carries a constant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Carrier {
    /// The displacement off a register; `negative` when the code reaches the
    /// value as `[reg - value]`.
    Displacement {
        /// Whether the displacement is the value negated.
        negative: bool,
    },
    /// The immediate.
    Immediate,
    /// The immediate as a shift count: the value is `1 << count`.
    Shift,
}

/// A constant's pattern and where its value lies in the match.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ConstantPattern {
    /// The pattern, as `code::Pattern` parses it.
    pub(crate) text: String,
    /// The operand, `(offset in the match, size)`; `None` when the pattern
    /// only confirms the value.
    pub(crate) operand: Option<(u64, u8)>,
    /// Whether the operand is the value negated.
    pub(crate) negative: bool,
    /// Whether the operand is a shift count.
    pub(crate) shift: bool,
}

/// The pattern of the symbol at `rva` from its own code, when the code
/// around it has a unique window.
pub(crate) fn symbol(exe: &Exe<'_>, rva: u64) -> Option<SymbolPattern> {
    let (bounds, masked) = decode(exe, rva)?;
    let offset = usize::try_from(rva - bounds.start).ok()?;
    let target = masked.insn_at(offset)?;
    let entry = rva == bounds.start || exe.function_at(rva).is_some_and(|f| f.start == rva);
    let window = unique_window(exe.text().0, &masked, target..target + 1, &[], &[], entry)?;
    Some(SymbolPattern {
        text: render(&window),
        locate: Locate::At(u64::try_from(offset - window.start).ok()?),
    })
}

/// The pattern of the symbol that the instruction at `site` calls, jumps to
/// or addresses RIP-relative, and the symbol's RVA.
///
/// # Errors
///
/// Why no such pattern can be made.
pub(crate) fn symbol_through(exe: &Exe<'_>, site: u64) -> anyhow::Result<(SymbolPattern, u64)> {
    let (masked, _, index) = instruction(exe, site)?;
    let insn = masked.insns[index];
    let (Some(reference), Some(rel)) = (insn.reference, insn.reference_at) else {
        bail!("{site:#x} neither calls nor addresses anything");
    };
    let window = unique_window(exe.text().0, &masked, index..index + 1, &[], &[], false)
        .with_context(|| format!("no unique window around {site:#x}"))?;
    let at = insn.offset + rel - window.start;
    let end = insn.offset + insn.len - window.start;
    let (Reference::Code(target) | Reference::Data(target)) = reference;
    Ok((
        SymbolPattern {
            text: render(&window),
            locate: Locate::Rel {
                at: u64::try_from(at)?,
                end: u64::try_from(end)?,
            },
        },
        target,
    ))
}

/// The pattern that reads a constant from the operand `carrier` of the
/// instruction at `site`.
///
/// # Errors
///
/// Why no such pattern can be made.
pub(crate) fn constant_from(
    exe: &Exe<'_>,
    site: u64,
    carrier: Carrier,
) -> anyhow::Result<ConstantPattern> {
    let (masked, _, index) = instruction(exe, site)?;
    let insn = masked.insns[index];
    let (at, width) =
        carried(&insn, carrier).with_context(|| format!("{site:#x} has no such operand"))?;
    let operand = insn.offset + at;
    let hide = operand..operand + width;
    let window = unique_window(
        exe.text().0,
        &masked,
        index..index + 1,
        core::slice::from_ref(&hide),
        &[],
        false,
    )
    .with_context(|| format!("no unique window around {site:#x}"))?;
    Ok(ConstantPattern {
        text: render(&window),
        operand: Some((u64::try_from(operand - window.start)?, u8::try_from(width)?)),
        negative: matches!(carrier, Carrier::Displacement { negative: true }),
        shift: carrier == Carrier::Shift,
    })
}

/// The pattern that confirms a constant by the exact instruction at `site`.
///
/// # Errors
///
/// Why no such pattern can be made.
pub(crate) fn constant_confirmed(exe: &Exe<'_>, site: u64) -> anyhow::Result<ConstantPattern> {
    let (masked, _, index) = instruction(exe, site)?;
    let window = unique_window(
        exe.text().0,
        &masked,
        index..index + 1,
        &[],
        &[index],
        false,
    )
    .with_context(|| format!("no unique window around {site:#x}"))?;
    Ok(ConstantPattern {
        text: render(&window),
        operand: None,
        negative: false,
        shift: false,
    })
}

/// The operand of `insn` that `carrier` names: its offset in the
/// instruction and size.
pub(crate) fn carried(insn: &Insn, carrier: Carrier) -> Option<(usize, usize)> {
    match carrier {
        Carrier::Displacement { .. } => insn.displacement_at,
        Carrier::Immediate | Carrier::Shift => insn.immediate_at,
    }
}

/// The operand of `insn` that carries `value`, if any.
pub(crate) fn carrier_of(insn: &Insn, value: u64) -> Option<Carrier> {
    [
        Carrier::Displacement { negative: false },
        Carrier::Immediate,
        Carrier::Displacement { negative: true },
    ]
    .into_iter()
    .find(|&carrier| value != 0 && value_of(insn, carrier) == Some(value))
}

/// The value the operand `carrier` of `insn` holds, as the loader reads it:
/// the operand's bytes zero-extended, or the displacement negated.
pub(crate) fn value_of(insn: &Insn, carrier: Carrier) -> Option<u64> {
    let raw = |value: Option<u64>, at: Option<(usize, usize)>| {
        let (_, size) = at?;
        let bits = u32::try_from(size * 8).ok()?;
        Some(value? & u64::MAX.checked_shr(64 - bits).unwrap_or(u64::MAX))
    };
    match carrier {
        Carrier::Displacement { negative: false } => raw(insn.displacement, insn.displacement_at),
        Carrier::Displacement { negative: true } => insn.displacement.map(u64::wrapping_neg),
        Carrier::Immediate => raw(insn.immediate, insn.immediate_at),
        Carrier::Shift => raw(insn.immediate, insn.immediate_at)
            .and_then(|count| 1u64.checked_shl(u32::try_from(count).ok()?)),
    }
}

/// The decoded function holding the instruction at `site`, where it starts,
/// and the instruction's index.
///
/// # Errors
///
/// When no instruction starts at `site`.
pub(crate) fn instruction(exe: &Exe<'_>, site: u64) -> anyhow::Result<(Masked, u64, usize)> {
    let (bounds, masked) = decode(exe, site).with_context(|| format!("no code at {site:#x}"))?;
    let index = usize::try_from(site - bounds.start)
        .ok()
        .and_then(|offset| masked.insn_at(offset))
        .with_context(|| format!("no instruction starts at {site:#x}"))?;
    Ok((masked, bounds.start, index))
}

/// The instruction whose bytes hold `rva`, in the code decoded around
/// `start`, an instruction at or before it such as a pattern's match: the
/// decoded code, where it starts, and the instruction's index.
pub(crate) fn instruction_holding(
    exe: &Exe<'_>,
    start: u64,
    rva: u64,
) -> Option<(Masked, u64, usize)> {
    let (bounds, masked) = decode(exe, start)?;
    let offset = usize::try_from(rva.checked_sub(bounds.start)?).ok()?;
    let index = masked
        .insns
        .iter()
        .position(|i| i.offset <= offset && offset < i.offset + i.len)?;
    Some((masked, bounds.start, index))
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

/// The smallest window of instructions around `core` that is unique in the
/// executable's code, with `hide` masked; the loose form (every field and
/// frame offset masked but in `keep`) when one is unique, else the strict
/// one. Only forward from `core` when `forward_only`.
fn unique_window(
    text: &[u8],
    masked: &Masked,
    core: Range<usize>,
    hide: &[Range<usize>],
    keep: &[usize],
    forward_only: bool,
) -> Option<Owned> {
    [true, false]
        .into_iter()
        .find_map(|loose| grow(text, masked, core.clone(), hide, loose, keep, forward_only))
}

fn grow(
    text: &[u8],
    masked: &Masked,
    core: Range<usize>,
    hide: &[Range<usize>],
    loose: bool,
    keep: &[usize],
    forward_only: bool,
) -> Option<Owned> {
    let count = masked.insns.len();
    let mut span = core.start..core.end.min(count);
    loop {
        let window = masked.window_hiding(span.clone(), hide, loose, keep)?;
        if window.bytes.len() > MAX_LEN {
            return None;
        }
        let pattern = window.pattern();
        if pattern.significant() >= MIN_SIGNIFICANT && pattern.find(text, 2).len() == 1 {
            return Some(window);
        }
        let can_back = !forward_only && span.start > 0;
        let can_on = span.end < count;
        if !can_back && !can_on {
            return None;
        }
        if can_on && (!can_back || span.end - core.end <= core.start - span.start) {
            span.end += 1;
        } else {
            span.start -= 1;
        }
    }
}

/// A symbol pinned by its whole function: the window that finds it, and
/// the function's body, which a table checks by its hash.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BodyPattern {
    /// The window from the function's first byte that finds it.
    pub(crate) window: SymbolPattern,
    /// The whole function, its moving parts open.
    pub(crate) body: Body,
}

/// The window that finds the function at `rva` and its body, so that any
/// change to its code, other than where it and what it reaches moved, fails
/// the body: for a symbol whose code the loader replaces or relies on as a
/// whole. The window starts at the function, or with `site` is the window
/// around the instruction there that calls, jumps to or addresses it, for a
/// function whose start has a twin.
///
/// # Errors
///
/// When the function does not decode to its end, no window is unique, or
/// `site` reaches another address.
pub(crate) fn body(exe: &Exe<'_>, rva: u64, site: Option<u64>) -> anyhow::Result<BodyPattern> {
    let (text, text_rva) = exe.text();
    let end = text_rva + text.len() as u64;
    let len = end.saturating_sub(rva).min(u64::from(Body::MAX_LEN));
    let code = exe
        .code(rva, len)
        .with_context(|| format!("{rva:#x} is not code"))?;
    let masked = Masked::decode(code, rva, &exe.absolute_range());
    let body = body_of(&masked).with_context(|| {
        format!("the function at {rva:#x} has no end, or a moving field not of 4 bytes")
    })?;
    let window = match site {
        Some(site) => {
            let (window, target) = symbol_through(exe, site)?;
            if target != rva {
                bail!("{site:#x} reaches {target:#x}, not {rva:#x}");
            }
            window
        }
        None => unique_window(text, &masked, 0..1, &[], &[], true)
            .filter(|window| window.bytes.len() * 2 <= body.len())
            .map(|window| SymbolPattern {
                text: render(&window),
                locate: Locate::At(0),
            })
            .or_else(|| preceded(exe, rva, &body))
            .with_context(|| {
                format!(
                    "no unique window over at most half of the function at {rva:#x}; pick `body:<rva>` of a call to it"
                )
            })?,
    };
    Ok(BodyPattern { window, body })
}

/// The smallest unique window in `text` from the start of the function
/// that starts `masked`, and the function's body with every masked field
/// open.
#[cfg(test)]
fn body_parts(masked: &Masked, text: &[u8]) -> Option<(Owned, Body)> {
    let body = body_of(masked)?;
    let window = unique_window(text, masked, 0..1, &[], &[], true)?;
    Some((window, body))
}

/// A window that ends in the first instructions of the function at `rva`
/// and starts in the code before it, for a short function whose own
/// window would cover most of it: at most half of `body` lies in the
/// window.
fn preceded(exe: &Exe<'_>, rva: u64, body: &Body) -> Option<SymbolPattern> {
    let functions = exe.functions();
    let before = functions.get(functions.partition_point(|f| f.end <= rva).checked_sub(1)?)?;
    let len = rva - before.start + body.len() as u64;
    let masked = Masked::decode(
        exe.code(before.start, len)?,
        before.start,
        &exe.absolute_range(),
    );
    let window = preceded_window(
        &masked,
        usize::try_from(rva - before.start).ok()?,
        body,
        exe.text().0,
    )?;
    Some(SymbolPattern {
        text: render(&window.0),
        locate: Locate::At(u64::try_from(window.1).ok()?),
    })
}

/// The smallest unique window in `text` that ends in the function at
/// `offset` of `masked` with at most half of `body` and starts as late as
/// it can before it, with how far before the function it starts.
fn preceded_window(
    masked: &Masked,
    offset: usize,
    body: &Body,
    text: &[u8],
) -> Option<(Owned, usize)> {
    let first = masked.insn_at(offset)?;
    let inside = |end: usize| {
        masked
            .insns
            .get(end - 1)
            .is_some_and(|last| (last.offset + last.len - offset) * 2 <= body.len())
    };
    let ends = (first + 1..=masked.insns.len()).take_while(|&end| inside(end));
    for end in ends {
        'starts: for start in (0..=first).rev() {
            for loose in [true, false] {
                let window = masked.window_hiding(start..end, &[], loose, &[])?;
                if window.bytes.len() > MAX_LEN {
                    break 'starts;
                }
                let pattern = window.pattern();
                if pattern.significant() >= MIN_SIGNIFICANT && pattern.find(text, 2).len() == 1 {
                    let before = offset - window.start;
                    return Some((window, before));
                }
            }
        }
    }
    None
}

/// The body of the function that starts `masked`, every masked field open.
fn body_of(masked: &Masked) -> Option<Body> {
    let last = masked.insns.get(masked.body_len()?.checked_sub(1)?)?;
    let len = last.offset + last.len;
    let mask = masked.mask.get(..len)?;
    let field = Body::FIELD as usize;
    let mut open = Vec::new();
    let mut at = 0;
    while at < len {
        if mask[at] {
            at += 1;
            continue;
        }
        let run = mask[at..].iter().take_while(|&&fixed| !fixed).count();
        if run % field != 0 {
            return None;
        }
        for start in (at..at + run).step_by(field) {
            open.push(u32::try_from(start).ok()?);
        }
        at += run;
    }
    Body::of(masked.bytes.get(..len)?, open)
}

/// The window as `code::Pattern` spells it: hex bytes, `??` for masked ones.
fn render(window: &Owned) -> String {
    let tokens: Vec<String> = window
        .bytes
        .iter()
        .zip(&window.mask)
        .map(|(byte, &fixed)| {
            if fixed {
                format!("{byte:02X}")
            } else {
                "??".to_owned()
            }
        })
        .collect();
    tokens.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOWHERE: Range<u64> = 0x1_4000_0000..0x2_4000_0000;

    fn insn(code: &[u8]) -> Insn {
        Masked::decode(code, 0x1000, &NOWHERE).insns[0]
    }

    /// A function of `count` distinct `mov eax, imm32` and a call, then
    /// `ret`.
    fn movs(count: u8) -> Vec<u8> {
        let mut code = vec![0xe8, 0x10, 0x20, 0x00, 0x00]; // call +0x2010
        for n in 0..count {
            code.extend_from_slice(&[0xb8, n, 0x5a, n, 0xa5]);
        }
        code.push(0xc3);
        code
    }

    #[test]
    fn a_body_covers_the_whole_function_and_its_window_only_finds_it() {
        let mut text = vec![0xcc; 8];
        let function = movs(40);
        text.extend_from_slice(&function);
        text.extend_from_slice(&[0xcc, 0xcc, 0x90]);
        let masked = Masked::decode(&text[8..], 0x1008, &NOWHERE);
        let (window, body) = body_parts(&masked, &text).unwrap();
        assert_eq!(body.len(), function.len());
        assert_eq!(body.open(), [1], "the call's target");
        assert!(body.holds(&function));
        let mut changed = function.clone();
        changed[100] ^= 1;
        assert!(!body.holds(&changed));
        assert_eq!(window.start, 0);
        assert!(
            window.bytes.len() < function.len(),
            "a window, not the code"
        );
        assert!(window.bytes.len() <= MAX_LEN);
        assert_eq!(window.pattern().find(&text, 2), [8]);
    }

    #[test]
    fn a_short_function_s_window_starts_in_the_code_before_it() {
        // sub rsp, 0x28; mov ecx, 0x44332211; add rsp, 0x28; ret
        let before = [
            0x48, 0x83, 0xec, 0x28, 0xb9, 0x11, 0x22, 0x33, 0x44, 0x48, 0x83, 0xc4, 0x28, 0xc3,
            0xcc, 0xcc,
        ];
        let function = movs(6);
        let mut text = function.clone();
        text.extend_from_slice(&[0xcc, 0xcc]);
        let at = text.len();
        text.extend_from_slice(&before);
        text.extend_from_slice(&function);
        text.push(0xcc);
        let body = Body::of(&function, vec![1]).unwrap();
        let masked = Masked::decode(&text[at..], 0x1000, &NOWHERE);
        let (window, ahead) = preceded_window(&masked, before.len(), &body, &text).unwrap();
        assert_eq!(window.pattern().find(&text, 2), [at + before.len() - ahead]);
        assert!(
            (window.bytes.len() - ahead) * 2 <= body.len(),
            "at most half of it"
        );
        assert!(ahead > 0);
    }

    #[test]
    fn a_body_with_a_twin_finds_it_by_the_bytes_after_it() {
        let leaf = movs(3);
        let mut text = Vec::new();
        text.extend_from_slice(&leaf);
        text.extend_from_slice(&[0xcc, 0xcc, 0x90]);
        let at = text.len();
        text.extend_from_slice(&leaf);
        text.extend_from_slice(&[0xcc, 0xcc, 0x33, 0xc0]);
        let masked = Masked::decode(&text[at..], 0x1000 + at as u64, &NOWHERE);
        let (window, body) = body_parts(&masked, &text).unwrap();
        assert_eq!(body.len(), leaf.len(), "the body ends at its return");
        assert!(window.bytes.len() > leaf.len());
        assert_eq!(window.pattern().find(&text, 2), [at]);
        let alone = Masked::decode(&leaf, 0x1000, &NOWHERE);
        assert!(body_parts(&alone, &[leaf.clone(), leaf].concat()).is_none());
    }

    #[test]
    fn an_operand_reads_as_the_loader_reads_it() {
        // mov ecx, [rax + 0x148]
        let field = insn(&[0x8b, 0x88, 0x48, 0x01, 0x00, 0x00]);
        assert_eq!(
            carrier_of(&field, 0x148),
            Some(Carrier::Displacement { negative: false })
        );
        assert_eq!(carried(&field, Carrier::Immediate), None);
        // lea rdx, [rbp - 0x60]
        let below = insn(&[0x48, 0x8d, 0x55, 0xa0]);
        assert_eq!(
            carrier_of(&below, 0x60),
            Some(Carrier::Displacement { negative: true })
        );
        // cmp eax, -1 as imm8: read as its byte, as the table reads it.
        let minus = insn(&[0x83, 0xf8, 0xff]);
        assert_eq!(value_of(&minus, Carrier::Immediate), Some(0xff));
        // shl r9, 6
        let shift = insn(&[0x49, 0xc1, 0xe1, 0x06]);
        assert_eq!(value_of(&shift, Carrier::Shift), Some(0x40));
        assert_eq!(carried(&shift, Carrier::Shift), Some((3, 1)));
        // mov word [rbx + 0x1c], 0x101: both operands, the displacement first.
        let both = insn(&[0x66, 0xc7, 0x43, 0x1c, 0x01, 0x01]);
        assert_eq!(
            carrier_of(&both, 0x1c),
            Some(Carrier::Displacement { negative: false })
        );
        assert_eq!(carrier_of(&both, 0x101), Some(Carrier::Immediate));
        assert_eq!(carrier_of(&both, 0), None);
    }
}
