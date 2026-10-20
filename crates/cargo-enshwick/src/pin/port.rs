//! Re-finds a pinned address of the old build in the new one.
//!
//! First the whole function: the function table entry that holds the
//! address, masked, must equal exactly one entry of the same length in the
//! new build. When it changed, the smallest window of instructions around the
//! address that is unique in the old build must occur exactly once in the new
//! one, inside a function, and at its start when the address is an entry.
//! Anything else is reported, never guessed.

use std::cell::OnceCell;
use std::fmt;
use std::ops::Range;

use super::exe::Exe;
use super::signature::{Masked, Pattern, Reference};

/// How many references to a symbol are followed at most.
const MAX_REFERENCES: usize = 32;

/// Windows smaller than this many significant bytes are not trusted.
const MIN_SIGNIFICANT: usize = 16;

/// How sure a match is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Confidence {
    /// The whole function is unchanged, apart from where it points.
    High,
    /// A unique stretch of code around the address, or a whole function told
    /// apart from its copies by what it calls and reads.
    Medium,
}

/// What became of a pinned address.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    /// Found, at the same address or a new one.
    Found {
        /// The new RVA.
        rva: u64,
        /// How sure.
        confidence: Confidence,
        /// How it was found.
        how: String,
    },
    /// The signature matches more than one place.
    Ambiguous(String),
    /// The signature matches nowhere, or no signature can be made.
    Missing(String),
}

impl Outcome {
    /// The new RVA, when found.
    pub(crate) fn rva(&self) -> Option<u64> {
        match self {
            Self::Found { rva, .. } => Some(*rva),
            Self::Ambiguous(_) | Self::Missing(_) => None,
        }
    }
}

impl fmt::Display for Confidence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::High => "high",
            Self::Medium => "medium",
        })
    }
}

/// A pinned address, ported: the outcome, and the old code it rests on.
#[derive(Debug)]
pub(crate) struct Ported {
    /// What became of it.
    pub(crate) outcome: Outcome,
    /// The old code around the address and where it starts.
    pub(crate) old_code: Option<(u64, Masked)>,
    /// The old RVAs proven unchanged by the match.
    pub(crate) proven: Option<Range<u64>>,
    /// For an ambiguous whole function, where each alike copy would put the
    /// address.
    pub(crate) candidates: Vec<u64>,
}

/// Ports addresses from one build to another.
pub(crate) struct Porter<'p, 'a> {
    old: &'p Exe<'a>,
    new: &'p Exe<'a>,
    /// Every reference in the old build's functions: `(target, site)`,
    /// sorted. Built on first need.
    references: OnceCell<Vec<(u64, u64)>>,
}

impl fmt::Debug for Porter<'_, '_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Porter").finish_non_exhaustive()
    }
}

impl<'p, 'a> Porter<'p, 'a> {
    /// A porter from `old` to `new`.
    pub(crate) fn new(old: &'p Exe<'a>, new: &'p Exe<'a>) -> Self {
        Self {
            old,
            new,
            references: OnceCell::new(),
        }
    }

    /// Re-finds `rva`: by its own code, else, when that is ambiguous or the
    /// address is data, through the code that references it.
    pub(crate) fn symbol(&self, rva: u64) -> Ported {
        let mut ported = direct(self.old, self.new, rva);
        // References point at a function's start or at the data itself.
        let (target, offset) = match &ported.old_code {
            Some((start, _)) => (*start, rva - start),
            None => (rva, 0),
        };
        match &ported.outcome {
            Outcome::Found { .. } => {}
            Outcome::Ambiguous(why) => {
                if let Some(outcome) = self.through_references(target, offset, &ported.candidates) {
                    ported.outcome = match outcome {
                        Ok(outcome) => {
                            // The found copy matches the whole old function.
                            if !ported.candidates.is_empty() {
                                ported.proven = ported
                                    .old_code
                                    .as_ref()
                                    .map(|(start, m)| *start..start + m.bytes.len() as u64);
                            }
                            outcome
                        }
                        Err(note) => Outcome::Ambiguous(format!("{why}; {note}")),
                    };
                }
            }
            Outcome::Missing(why) => {
                let in_code = ported.old_code.is_some();
                match self.through_references(target, offset, &[]) {
                    // Data is found only through its references.
                    Some(Ok(outcome)) if !in_code => ported.outcome = outcome,
                    Some(Ok(Outcome::Found { rva: hint, .. })) => {
                        ported.outcome = Outcome::Missing(format!(
                            "{why}; its references now point at {hint:#x}: check by hand"
                        ));
                    }
                    Some(Err(note)) => ported.outcome = Outcome::Missing(format!("{why}; {note}")),
                    _ => {}
                }
            }
        }
        ported
    }

    /// Follows the old build's references to `target` into the new build.
    /// `None` when no reference could be followed; `Err` when they disagree.
    fn through_references(
        &self,
        target: u64,
        offset: u64,
        candidates: &[u64],
    ) -> Option<Result<Outcome, String>> {
        let references = self.references.get_or_init(|| references(self.old));
        let from = references.partition_point(|&(t, _)| t < target);
        let sites: Vec<u64> = references
            .get(from..)?
            .iter()
            .take_while(|&&(t, _)| t == target)
            .take(MAX_REFERENCES)
            .map(|&(_, site)| site)
            .collect();
        let mut seen: Vec<u64> = Vec::new();
        for &site in &sites {
            let Some(new_site) = direct(self.old, self.new, site).outcome.rva() else {
                continue;
            };
            let Some(code) = self.new.code(new_site, 15) else {
                continue;
            };
            let insn = Masked::decode(code, new_site, &self.new.absolute_range());
            let Some(Reference::Code(new_target) | Reference::Data(new_target)) =
                insn.insns.first().and_then(|i| i.reference)
            else {
                continue;
            };
            if !seen.contains(&new_target) {
                seen.push(new_target);
            }
        }
        match seen.as_slice() {
            [] => None,
            [only] => {
                let rva = only + offset;
                if !candidates.is_empty() && !candidates.contains(&rva) {
                    return Some(Err(format!(
                        "its references point at {rva:#x}, which is none of the copies"
                    )));
                }
                Some(Ok(Outcome::Found {
                    rva,
                    confidence: Confidence::Medium,
                    how: format!("through its references ({} followed)", sites.len()),
                }))
            }
            many => Some(Err(format!(
                "its references point at {} different places",
                many.len()
            ))),
        }
    }
}

/// Every reference the old build's functions make, as `(target, site)`.
fn references(exe: &Exe<'_>) -> Vec<(u64, u64)> {
    let absolute = exe.absolute_range();
    let mut out = Vec::new();
    for function in exe.functions() {
        let Some(code) = exe.code(function.start, function.end - function.start) else {
            continue;
        };
        let masked = Masked::decode(code, function.start, &absolute);
        out.extend(masked.insns.iter().filter_map(|insn| match insn.reference {
            Some(Reference::Code(t) | Reference::Data(t)) => {
                Some((t, function.start + insn.offset as u64))
            }
            None => None,
        }));
    }
    out.sort_unstable();
    out.dedup();
    out
}

/// Re-finds `rva` of `old` in `new` by its own code.
fn direct(old: &Exe<'_>, new: &Exe<'_>, rva: u64) -> Ported {
    let missing = |why: &str| Ported {
        outcome: Outcome::Missing(why.to_owned()),
        old_code: None,
        proven: None,
        candidates: Vec::new(),
    };
    let Some((bounds, in_table)) = old.code_around(rva) else {
        return missing("not in the old build's code");
    };
    let Some(code) = old.code(bounds.start, bounds.end - bounds.start) else {
        return missing("not in the old build's code");
    };
    let mut masked = Masked::decode(code, bounds.start, &old.absolute_range());
    if !in_table {
        // A leaf function: it ends at the padding before the next one.
        masked.cut_at_padding();
    }
    let offset = usize::try_from(rva - bounds.start).unwrap_or(usize::MAX);
    let Some(target) = masked.insn_at(offset) else {
        return Ported {
            outcome: Outcome::Missing("not at an instruction boundary".to_owned()),
            old_code: Some((bounds.start, masked)),
            proven: None,
            candidates: Vec::new(),
        };
    };
    let whole = masked.bytes.len() as u64 == bounds.end - bounds.start;
    let mut candidates = Vec::new();
    let (outcome, proven) = if in_table && whole {
        match whole_function(old, new, &bounds, &masked) {
            Some((
                Outcome::Found {
                    rva: start,
                    confidence,
                    how,
                },
                _,
            )) => (
                Outcome::Found {
                    rva: start + (rva - bounds.start),
                    confidence,
                    how,
                },
                Some(bounds.clone()),
            ),
            Some((other, alike)) => {
                candidates = alike.iter().map(|s| s + (rva - bounds.start)).collect();
                (other, None)
            }
            None => window(old, new, rva, &bounds, &masked, target, in_table),
        }
    } else {
        window(old, new, rva, &bounds, &masked, target, in_table)
    };
    Ported {
        outcome,
        old_code: Some((bounds.start, masked)),
        proven,
        candidates,
    }
}

/// The whole function, among the new build's entries of the same length,
/// with the starts of the alike copies when it is ambiguous. `None` when no
/// entry matches.
fn whole_function(
    old: &Exe<'_>,
    new: &Exe<'_>,
    bounds: &Range<u64>,
    masked: &Masked,
) -> Option<(Outcome, Vec<u64>)> {
    let pattern = masked.pattern();
    let len = bounds.end - bounds.start;
    let candidates: Vec<u64> = new
        .functions_of_len(len)
        .filter(|f| new.code(f.start, len).is_some_and(|c| pattern.matches(c)))
        .map(|f| f.start)
        .collect();
    match candidates.as_slice() {
        [] => None,
        [only] => Some((
            Outcome::Found {
                rva: *only,
                confidence: Confidence::High,
                how: "whole function".to_owned(),
            },
            Vec::new(),
        )),
        many => {
            let want = referents(old, masked);
            let alike: Vec<u64> = many
                .iter()
                .copied()
                .filter(|&start| {
                    new.code(start, len).is_some_and(|code| {
                        referents(new, &Masked::decode(code, start, &new.absolute_range())) == want
                    })
                })
                .collect();
            let outcome = match alike.as_slice() {
                [only] => Outcome::Found {
                    rva: *only,
                    confidence: Confidence::Medium,
                    how: format!(
                        "whole function, told from {} copies by what it calls and reads",
                        many.len() - 1
                    ),
                },
                _ => Outcome::Ambiguous(format!(
                    "the whole function matches {} places; {} alike in what they call and read",
                    many.len(),
                    alike.len()
                )),
            };
            Some((outcome, alike))
        }
    }
}

/// What the code calls and reads: the masked start of each call target and
/// the first bytes of each RIP-relative operand.
fn referents(exe: &Exe<'_>, masked: &Masked) -> Vec<Option<Vec<u8>>> {
    masked
        .insns
        .iter()
        .filter_map(|insn| insn.reference)
        .map(|reference| match reference {
            Reference::Code(target) => exe.code(target, 32).map(|code| {
                let callee = Masked::decode(code, target, &exe.absolute_range());
                callee
                    .bytes
                    .iter()
                    .zip(&callee.mask)
                    .map(|(&b, &m)| if m { b } else { 0 })
                    .collect()
            }),
            Reference::Data(target) => exe.data(target, 16).map(<[u8]>::to_vec),
        })
        .collect()
}

/// The smallest unique window around the target instruction.
fn window(
    old: &Exe<'_>,
    new: &Exe<'_>,
    rva: u64,
    bounds: &Range<u64>,
    masked: &Masked,
    target: usize,
    in_table: bool,
) -> (Outcome, Option<Range<u64>>) {
    let entry = in_table && rva == bounds.start;
    let forward_only = entry || !in_table;
    let (old_text, old_text_rva) = old.text();
    let (new_text, new_text_rva) = new.text();
    let count = masked.insns.len();
    let mut span = target..(target + 1).min(count);
    let mut want = MIN_SIGNIFICANT;
    let mut last_new = 0;
    loop {
        // Grow until the window holds `want` significant bytes.
        while span_pattern(masked, &span).is_none_or(|p| p.significant() < want) {
            let can_back = !forward_only && span.start > 0;
            let can_on = span.end < count;
            if !can_back && !can_on {
                break;
            }
            if can_on && (!can_back || (span.end - target) <= (target - span.start)) {
                span.end += 1;
            } else {
                span.start -= 1;
            }
        }
        let Some(pattern) = span_pattern(masked, &span) else {
            return (Outcome::Missing("no code to sign".to_owned()), None);
        };
        let grown_fully = (forward_only || span.start == 0) && span.end == count;
        if pattern.significant() >= MIN_SIGNIFICANT {
            let start_offset = masked.insns.get(span.start).map_or(0, |i| i.offset) as u64;
            let window_rva = bounds.start + start_offset;
            let old_hits = pattern.find(old_text, 2);
            let unique_in_old = old_hits.len() == 1
                && old_hits.first().map(|&h| old_text_rva + h as u64) == Some(window_rva);
            if unique_in_old {
                let hits = pattern.find(new_text, 3);
                last_new = hits.len();
                match hits.as_slice() {
                    [] => {
                        return (
                            Outcome::Missing(format!(
                                "the code around it changed ({} bytes signed)",
                                pattern.bytes.len()
                            )),
                            None,
                        );
                    }
                    [hit] => {
                        let new_rva = new_text_rva + *hit as u64 + (rva - window_rva);
                        let delta = (new_text_rva + *hit as u64).wrapping_sub(window_rva);
                        let old_range = unchanged_around(
                            masked,
                            bounds.start,
                            window_rva..window_rva + pattern.bytes.len() as u64,
                            |rva| {
                                new.code(rva.wrapping_add(delta), 1)
                                    .and_then(|b| b.first().copied())
                            },
                        );
                        return (
                            verify(new, new_rva, entry, in_table, pattern),
                            Some(old_range),
                        );
                    }
                    _ => {}
                }
            }
        }
        if grown_fully {
            break;
        }
        want *= 2;
    }
    let why = if last_new > 1 {
        format!("the largest signature matches {last_new} places or more")
    } else {
        "no signature around it is unique in the old build".to_owned()
    };
    (Outcome::Ambiguous(why), None)
}

/// Widens `proven`, old RVAs whose code matched, over the neighbouring old
/// bytes that still match the new code at the same distance.
fn unchanged_around(
    masked: &Masked,
    start: u64,
    proven: Range<u64>,
    new_byte: impl Fn(u64) -> Option<u8>,
) -> Range<u64> {
    let same = |rva: u64| {
        let i = usize::try_from(rva - start).unwrap_or(usize::MAX);
        match (masked.bytes.get(i), masked.mask.get(i)) {
            (Some(_), Some(false)) => true,
            (Some(&want), Some(true)) => new_byte(rva) == Some(want),
            _ => false,
        }
    };
    let mut range = proven;
    while range.start > start && same(range.start - 1) {
        range.start -= 1;
    }
    while same(range.end) {
        range.end += 1;
    }
    range
}

fn span_pattern<'m>(masked: &'m Masked, span: &Range<usize>) -> Option<Pattern<'m>> {
    masked.window(span.clone())
}

/// Checks a window match against the new build's function bounds.
fn verify(new: &Exe<'_>, rva: u64, entry: bool, in_table: bool, pattern: Pattern<'_>) -> Outcome {
    let function = new.function_at(rva);
    let how = format!("unique {} byte signature", pattern.bytes.len());
    match (function, entry, in_table) {
        (Some(f), true, _) if f.start != rva => Outcome::Missing(format!(
            "the signature matches inside a function at {:#x}, not at its start",
            f.start
        )),
        (None, _, true) => {
            Outcome::Missing("the signature matches outside the new function table".to_owned())
        }
        _ => Outcome::Found {
            rva,
            confidence: Confidence::Medium,
            how,
        },
    }
}
