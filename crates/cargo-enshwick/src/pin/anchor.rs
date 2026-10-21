//! Anchors: the instruction a table entry's pattern is built around, chosen
//! once by a human (`--from`) and kept from then on.
//!
//! A constant read from an operand and a symbol reached through a rel32
//! keep their instruction: on the build a table is for, the entry's pattern
//! finds it again, and when the table is ported, the instruction is ported
//! like any address and read on the new build.

use std::collections::HashMap;

use enshwick_loader_builds::{Locate, Table};

use super::exe::Exe;
use super::patterns::{self, Carrier};
use super::signature::Reference;

/// The instruction an entry's pattern is built around.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Anchor {
    /// A constant read from the operand of the instruction, plus an
    /// addend.
    Read(u64, Carrier, i64),
    /// A constant that the exact instruction confirms.
    Confirm(u64),
    /// A symbol that the instruction calls, jumps to or addresses.
    Through(u64),
    /// A symbol found by its own code.
    Own,
}

impl Anchor {
    /// The instruction it rests on.
    pub(crate) const fn site(&self) -> Option<u64> {
        match self {
            Self::Read(site, ..) | Self::Confirm(site) | Self::Through(site) => Some(*site),
            Self::Own => None,
        }
    }

    /// The same anchor at another instruction.
    pub(crate) const fn moved_to(&self, site: u64) -> Self {
        match *self {
            Self::Read(_, carrier, plus) => Self::Read(site, carrier, plus),
            Self::Confirm(_) => Self::Confirm(site),
            Self::Through(_) => Self::Through(site),
            Self::Own => Self::Own,
        }
    }
}

/// Reads a `--from` pick for the entry whose value is `value`: for a
/// constant `<rva>`, optionally prefixed `disp:`, `neg:`, `imm:` or `shl:` and
/// followed by `+<n>` or `-<n>` for a value next to the operand, or
/// `confirm:<rva>`; for a symbol `<rva>`, `ref:<rva>` or `direct`.
///
/// # Errors
///
/// When the pick does not read or does not fit the code.
pub(crate) fn parse_pick(
    text: &str,
    value: u64,
    symbol: bool,
    exe: &Exe<'_>,
) -> Result<Anchor, String> {
    let number = |t: &str| {
        let digits = t.trim().trim_start_matches("0x").replace('_', "");
        u64::from_str_radix(&digits, 16).map_err(|_| format!("{t:?} is no hex RVA"))
    };
    if text == "direct" {
        return Ok(Anchor::Own);
    }
    if let Some(rest) = text.strip_prefix("confirm:") {
        return Ok(Anchor::Confirm(number(rest)?));
    }
    if let Some(rest) = text.strip_prefix("ref:") {
        return Ok(Anchor::Through(number(rest)?));
    }
    if symbol {
        let site = number(text)?;
        return Ok(if site == value {
            Anchor::Own
        } else {
            Anchor::Through(site)
        });
    }
    let (text, plus) = match text.rfind(['+', '-']) {
        Some(at) => {
            let n = i64::try_from(number(&text[at + 1..])?).map_err(|e| e.to_string())?;
            (&text[..at], if &text[at..=at] == "-" { -n } else { n })
        }
        None => (text, 0),
    };
    let part = value.wrapping_sub_signed(plus);
    let (kind, rva) = text.split_once(':').unwrap_or(("", text));
    let site = number(rva)?;
    let (masked, _, index) = patterns::instruction(exe, site)?;
    let insn = masked.insns[index];
    let carrier = match kind {
        "disp" => Carrier::Displacement { negative: false },
        "neg" => Carrier::Displacement { negative: true },
        "imm" => Carrier::Immediate,
        "shl" => Carrier::Shift,
        "" => patterns::carrier_of(&insn, part)
            .ok_or_else(|| format!("{site:#x} carries no {part:#x}"))?,
        other => return Err(format!("an unknown kind {other:?}")),
    };
    if patterns::value_of(&insn, carrier) != Some(part) {
        return Err(format!("{site:#x} does not carry {part:#x} there"));
    }
    Ok(Anchor::Read(site, carrier, plus))
}

/// The anchors of the entries of `table` that keep one, found in `exe` by
/// their patterns: constants read from an operand and symbols reached through a
/// rel32. Entries whose pattern does not match once are left out.
pub(crate) fn of_table(table: &Table, exe: &Exe<'_>) -> HashMap<String, Anchor> {
    let (code, code_rva) = exe.text();
    let mut out = HashMap::new();
    for constant in table.constants() {
        let (Some(pattern), Some(operand)) = (constant.pattern(), constant.operand()) else {
            continue;
        };
        let Ok(start) = pattern.find_unique(code) else {
            continue;
        };
        let start = code_rva + start as u64;
        let rva = start + operand.at();
        let Some((masked, function, index)) = patterns::instruction_holding(exe, start, rva) else {
            continue;
        };
        let insn = masked.insns[index];
        let Some(at) = usize::try_from(rva - function)
            .ok()
            .and_then(|o| o.checked_sub(insn.offset))
        else {
            continue;
        };
        let carrier = if insn.displacement_at.map(|(o, _)| o) == Some(at) {
            Carrier::Displacement {
                negative: operand.is_negative(),
            }
        } else if insn.immediate_at.map(|(o, _)| o) == Some(at) {
            if operand.is_shift() {
                Carrier::Shift
            } else {
                Carrier::Immediate
            }
        } else {
            continue;
        };
        let site = function + insn.offset as u64;
        out.insert(
            constant.id().to_owned(),
            Anchor::Read(site, carrier, operand.addend()),
        );
    }
    for symbol in table.symbols() {
        let Some((pattern, Locate::Rel { at, .. })) = symbol.pattern() else {
            continue;
        };
        let Ok(start) = pattern.find_unique(code) else {
            continue;
        };
        let start = code_rva + start as u64;
        if let Some((masked, function, index)) =
            patterns::instruction_holding(exe, start, start + at)
        {
            let site = function + masked.insns[index].offset as u64;
            out.insert(symbol.id().to_owned(), Anchor::Through(site));
        }
    }
    out
}

/// The constant the anchor reads in `exe`.
pub(crate) fn read(exe: &Exe<'_>, anchor: &Anchor) -> Option<u64> {
    let Anchor::Read(site, carrier, plus) = anchor else {
        return None;
    };
    let (masked, _, index) = patterns::instruction(exe, *site).ok()?;
    patterns::value_of(&masked.insns[index], *carrier)?.checked_add_signed(*plus)
}

/// Where the instruction at `site` in `exe` points.
pub(crate) fn target(exe: &Exe<'_>, site: u64) -> Option<u64> {
    let (masked, _, index) = patterns::instruction(exe, site).ok()?;
    match masked.insns[index].reference? {
        Reference::Code(rva) | Reference::Data(rva) => Some(rva),
    }
}
