//! Byte signatures of code, with the parts that change when code moves
//! masked: RIP-relative displacements, rel32 branch and call targets, and
//! absolute pointers.

use std::ops::Range;

use iced_x86::{Decoder, DecoderOptions, FlowControl, Instruction, OpKind, Register};
use memchr::memmem;

/// What an instruction points at, as an RVA.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Reference {
    /// A rel32 branch or call target.
    Code(u64),
    /// A RIP-relative memory operand.
    Data(u64),
}

/// One decoded instruction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Insn {
    /// Offset from the start of the code.
    pub(crate) offset: usize,
    /// Length in bytes.
    pub(crate) len: usize,
    /// A memory displacement that is not RIP-relative, sign extended.
    pub(crate) displacement: Option<u64>,
    /// An immediate that is not a branch target or a pointer.
    pub(crate) immediate: Option<u64>,
    /// Where the instruction points, when it does.
    pub(crate) reference: Option<Reference>,
}

impl Insn {
    /// Whether the instruction carries `value` as a displacement (either
    /// sign) or an immediate.
    pub(crate) fn uses(&self, value: u64) -> bool {
        self.displacement
            .is_some_and(|d| d == value || d.wrapping_neg() == value)
            || self.immediate == Some(value)
    }
}

/// Code with its signature: `mask[i]` is true when byte `i` must match.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Masked {
    /// The code bytes, cut at the first byte that does not decode.
    pub(crate) bytes: Vec<u8>,
    /// Which bytes are significant.
    pub(crate) mask: Vec<bool>,
    /// The instructions, in order.
    pub(crate) insns: Vec<Insn>,
}

impl Masked {
    /// Decodes `code`, which starts at `rva`. Immediates inside `absolute`
    /// are pointers into the image and are masked.
    pub(crate) fn decode(code: &[u8], rva: u64, absolute: &Range<u64>) -> Self {
        let mut decoder = Decoder::with_ip(64, code, rva, DecoderOptions::NONE);
        let mut instruction = Instruction::default();
        let mut mask = Vec::with_capacity(code.len());
        let mut insns = Vec::new();
        while decoder.can_decode() {
            let offset = decoder.position();
            decoder.decode_out(&mut instruction);
            if instruction.is_invalid() {
                break;
            }
            let len = instruction.len();
            let offsets = decoder.get_constant_offsets(&instruction);
            let start = mask.len();
            mask.resize(start + len, true);
            let mut hide = |at: usize, size: usize| {
                for byte in mask.iter_mut().skip(start + at).take(size) {
                    *byte = false;
                }
            };
            let mut insn = Insn {
                offset,
                len,
                displacement: None,
                immediate: None,
                reference: None,
            };
            if offsets.has_displacement() {
                if instruction.is_ip_rel_memory_operand() {
                    hide(offsets.displacement_offset(), offsets.displacement_size());
                    insn.reference = Some(Reference::Data(instruction.ip_rel_memory_address()));
                } else if instruction.memory_base() != Register::None
                    || instruction.memory_index() != Register::None
                {
                    insn.displacement = Some(instruction.memory_displacement64());
                }
            }
            if offsets.has_immediate() {
                if is_branch(&instruction) {
                    if offsets.immediate_size() == 4 {
                        hide(offsets.immediate_offset(), 4);
                        insn.reference = Some(Reference::Code(instruction.near_branch_target()));
                    }
                } else {
                    let value = immediate(&instruction);
                    if offsets.immediate_size() == 8 && absolute.contains(&value) {
                        hide(offsets.immediate_offset(), 8);
                    } else {
                        insn.immediate = Some(value);
                    }
                }
            }
            insns.push(insn);
        }
        let bytes = code.get(..mask.len()).unwrap_or_default().to_vec();
        Self { bytes, mask, insns }
    }

    /// The signature of the instructions `insns`.
    pub(crate) fn window(&self, insns: Range<usize>) -> Option<Pattern<'_>> {
        let first = self.insns.get(insns.start)?;
        let last = self.insns.get(insns.end.checked_sub(1)?)?;
        let bytes = first.offset..last.offset + last.len;
        Some(Pattern {
            bytes: self.bytes.get(bytes.clone())?,
            mask: self.mask.get(bytes)?,
        })
    }

    /// The signature of all the code.
    pub(crate) fn pattern(&self) -> Pattern<'_> {
        Pattern {
            bytes: &self.bytes,
            mask: &self.mask,
        }
    }

    /// Drops everything from the first `int3`, the padding between
    /// functions.
    pub(crate) fn cut_at_padding(&mut self) {
        if let Some(at) = self
            .insns
            .iter()
            .position(|i| self.bytes.get(i.offset) == Some(&0xcc))
        {
            let end = self.insns.get(at).map_or(self.bytes.len(), |i| i.offset);
            self.insns.truncate(at);
            self.bytes.truncate(end);
            self.mask.truncate(end);
        }
    }

    /// The index of the instruction that starts at `offset`.
    pub(crate) fn insn_at(&self, offset: usize) -> Option<usize> {
        self.insns.iter().position(|i| i.offset == offset)
    }
}

fn is_branch(instruction: &Instruction) -> bool {
    matches!(
        instruction.flow_control(),
        FlowControl::Call
            | FlowControl::UnconditionalBranch
            | FlowControl::ConditionalBranch
            | FlowControl::XbeginXabortXend
    )
}

fn immediate(instruction: &Instruction) -> u64 {
    (0..instruction.op_count())
        .find(|&i| {
            matches!(
                instruction.op_kind(i),
                OpKind::Immediate8
                    | OpKind::Immediate8to16
                    | OpKind::Immediate8to32
                    | OpKind::Immediate8to64
                    | OpKind::Immediate16
                    | OpKind::Immediate32
                    | OpKind::Immediate32to64
                    | OpKind::Immediate64
            )
        })
        .map_or(0, |i| instruction.immediate(i))
}

/// A byte signature: bytes and which of them must match.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Pattern<'a> {
    /// The bytes.
    pub(crate) bytes: &'a [u8],
    /// Which bytes must match.
    pub(crate) mask: &'a [bool],
}

impl Pattern<'_> {
    /// How many bytes must match.
    pub(crate) fn significant(&self) -> usize {
        self.mask.iter().filter(|&&m| m).count()
    }

    /// Whether the pattern matches `code` in full.
    pub(crate) fn matches(&self, code: &[u8]) -> bool {
        code.len() == self.bytes.len()
            && self
                .bytes
                .iter()
                .zip(self.mask)
                .zip(code)
                .all(|((want, &must), got)| !must || want == got)
    }

    /// Up to `limit` offsets in `hay` where the pattern matches.
    pub(crate) fn find(&self, hay: &[u8], limit: usize) -> Vec<usize> {
        let Some(anchor) = self.anchor() else {
            return Vec::new();
        };
        let Some(needle) = self.bytes.get(anchor.clone()) else {
            return Vec::new();
        };
        let finder = memmem::Finder::new(needle);
        let mut found = Vec::new();
        let mut from = anchor.start;
        while found.len() < limit {
            let Some(hit) = hay.get(from..).and_then(|rest| finder.find(rest)) else {
                break;
            };
            let at = from + hit;
            let start = at - anchor.start;
            if hay
                .get(start..start + self.bytes.len())
                .is_some_and(|code| self.matches(code))
            {
                found.push(start);
            }
            from = at + 1;
        }
        found
    }

    /// The longest run of significant bytes.
    fn anchor(&self) -> Option<Range<usize>> {
        let mut best: Option<Range<usize>> = None;
        let mut run_start = None;
        for (i, &must) in self.mask.iter().chain([&false]).enumerate() {
            match (must, run_start) {
                (true, None) => run_start = Some(i),
                (false, Some(start)) => {
                    if best.as_ref().is_none_or(|b| i - start > b.len()) {
                        best = Some(start..i);
                    }
                    run_start = None;
                }
                _ => {}
            }
        }
        best
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOWHERE: Range<u64> = 0x1_4000_0000..0x2_4000_0000;

    /// `mov rax, [rip+0x100]; call rel32; mov ecx, [rbx+0x178]; jne short;
    /// mov rdx, imm64 (a pointer); ret`.
    const CODE: &[u8] = &[
        0x48, 0x8b, 0x05, 0x00, 0x01, 0x00, 0x00, // mov rax, [rip+0x100]
        0xe8, 0x10, 0x20, 0x00, 0x00, // call +0x2010
        0x8b, 0x8b, 0x78, 0x01, 0x00, 0x00, // mov ecx, [rbx+0x178]
        0x75, 0x02, // jne +2
        0x48, 0xba, 0x00, 0x10, 0x00, 0x40, 0x01, 0x00, 0x00, 0x00, // mov rdx, 0x140001000
        0xc3, // ret
    ];

    #[test]
    fn moving_parts_are_masked_and_the_rest_kept() {
        let masked = Masked::decode(CODE, 0x1000, &NOWHERE);
        assert_eq!(masked.bytes, CODE);
        let hidden: Vec<usize> = (0..CODE.len()).filter(|&i| !masked.mask[i]).collect();
        // The RIP displacement, the call target and the pointer.
        let mut want: Vec<usize> = (3..7).chain(8..12).chain(22..30).collect();
        want.sort_unstable();
        assert_eq!(hidden, want);
        assert_eq!(masked.insns.len(), 6);
        assert_eq!(
            masked.insns[0].reference,
            Some(Reference::Data(0x1000 + 7 + 0x100))
        );
        assert_eq!(
            masked.insns[1].reference,
            Some(Reference::Code(0x1000 + 12 + 0x2010))
        );
        assert!(masked.insns[2].uses(0x178));
        assert_eq!(masked.insns[3].reference, None, "a short jump stays");
    }

    #[test]
    fn negative_displacements_count_as_their_magnitude() {
        // lea rdx, [rbp-0x60]
        let masked = Masked::decode(&[0x48, 0x8d, 0x55, 0xa0], 0, &NOWHERE);
        assert!(masked.insns[0].uses(0x60));
    }

    #[test]
    fn decoding_stops_at_bytes_that_are_no_code() {
        let masked = Masked::decode(&[0xc3, 0x0f, 0x0b, 0x06], 0, &NOWHERE);
        // ret; ud2; then 0x06 (push es) is invalid in 64-bit mode.
        assert_eq!(masked.bytes, [0xc3, 0x0f, 0x0b]);
    }

    #[test]
    fn a_leaf_function_ends_at_its_padding() {
        let mut masked = Masked::decode(&[0x31, 0xc0, 0xc3, 0xcc, 0xcc, 0x90], 0, &NOWHERE);
        masked.cut_at_padding();
        assert_eq!(masked.bytes, [0x31, 0xc0, 0xc3]);
        assert_eq!(masked.insns.len(), 2);
    }

    #[test]
    fn a_moved_copy_matches_its_signature() {
        let masked = Masked::decode(CODE, 0x1000, &NOWHERE);
        let mut moved = CODE.to_vec();
        moved[3..7].copy_from_slice(&0x5555u32.to_le_bytes());
        moved[8..12].copy_from_slice(&0x7777u32.to_le_bytes());
        assert!(masked.pattern().matches(&moved));
        moved[14] = 0x80;
        assert!(
            !masked.pattern().matches(&moved),
            "a changed offset does not"
        );
    }

    #[test]
    fn search_finds_every_match_including_overlapping_ones() {
        let bytes = [0xaa, 0xaa, 0x00];
        let mask = [true, true, false];
        let pattern = Pattern {
            bytes: &bytes,
            mask: &mask,
        };
        let hay = [0xaa, 0xaa, 0xaa, 0xaa, 0x01];
        assert_eq!(pattern.find(&hay, 10), vec![0, 1, 2]);
        assert_eq!(pattern.find(&hay, 2), vec![0, 1]);
    }

    #[test]
    fn a_unique_window_is_found_once_among_noise() {
        let masked = Masked::decode(CODE, 0x1000, &NOWHERE);
        let mut hay = vec![0xcc; 0x100];
        hay.extend_from_slice(&masked.bytes);
        hay.extend_from_slice(&[0xcc; 0x40]);
        let window = masked.window(1..4).unwrap();
        assert_eq!(window.find(&hay, 3), vec![0x100 + 7]);
        assert_eq!(window.significant(), 1 + 6 + 2);
        assert_eq!(masked.insn_at(12), Some(2));
        assert_eq!(masked.insn_at(13), None);
    }

    #[test]
    fn a_pattern_of_only_masked_bytes_finds_nothing() {
        let pattern = Pattern {
            bytes: &[1, 2],
            mask: &[false, false],
        };
        assert!(pattern.find(&[1, 2, 3], 5).is_empty());
    }
}
