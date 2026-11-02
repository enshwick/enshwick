//! The game's code as functions: their extents from the exception directory
//! (`.pdata`), each with the fragments the compiler split off it, decoded on
//! demand.

use core::ops::Range;

use iced_x86::{Decoder, DecoderOptions, Instruction, Mnemonic, OpKind};
use rustc_hash::FxHashSet;

use crate::reflect::Image;

/// A `RUNTIME_FUNCTION`: begin, end and unwind info RVAs.
const RUNTIME_FUNCTION: usize = 12;
/// `UNW_FLAG_CHAININFO` in the flags of an unwind info's first byte: the
/// fragment's unwind info ends in its parent's `RUNTIME_FUNCTION`.
const CHAIN_INFO: u8 = 0x4;
/// Chained fragments deeper than this are taken for a loop in bad data.
const MAX_CHAIN: usize = 8;
/// `lea r, [rip + disp32]`: the opcode and the `ModRM` bits of a RIP-relative
/// operand, which every string load the routes follow uses.
const LEA: u8 = 0x8d;
const MODRM_RIP: (u8, u8) = (0xc7, 0x05);

/// The code section of an image, indexed by function.
pub(crate) struct Code<'i, 'a> {
    image: &'i Image<'a>,
    base: u64,
    text: Range<u64>,
    /// Every function and fragment as `(begin, end)` RVAs, sorted.
    parts: Vec<(u32, u32)>,
    /// For each part, the begin RVA of the function it belongs to.
    primary: Vec<u32>,
    /// Every part as `(function begin, part begin, part end)`, sorted, so a
    /// function's parts lie together with its own first.
    by_function: Vec<(u32, u32, u32)>,
}

impl<'i, 'a> Code<'i, 'a> {
    /// The functions `image` declares; `None` without `.text` or `.pdata`.
    pub(crate) fn new(image: &'i Image<'a>) -> Option<Self> {
        let base = image.base();
        let text = image.range(image.section(".text")?);
        let pdata = image.range(image.section(".pdata")?);
        let table = image.read(pdata.start, pdata.end - pdata.start)?;
        let mut parts: Vec<(u32, u32, u32)> = table
            .as_chunks::<RUNTIME_FUNCTION>()
            .0
            .iter()
            .map(|f| {
                let word = |at: usize| u32::from_le_bytes([f[at], f[at + 1], f[at + 2], f[at + 3]]);
                (word(0), word(4), word(8))
            })
            .filter(|&(begin, end, _)| begin != 0 && begin < end)
            .collect();
        parts.sort_unstable();
        let primary: Vec<u32> = parts
            .iter()
            .map(|&(begin, _, unwind)| primary_of(image, base, begin, unwind))
            .collect();
        let mut by_function: Vec<(u32, u32, u32)> = parts
            .iter()
            .zip(&primary)
            .map(|(&(b, e, _), &function)| (function, b, e))
            .collect();
        // A function's own part sorts first: its begin is the function's.
        by_function.sort_unstable_by_key(|&(f, b, _)| (f, b != f, b));
        Some(Self {
            image,
            base,
            text,
            parts: parts.into_iter().map(|(b, e, _)| (b, e)).collect(),
            primary,
            by_function,
        })
    }

    pub(crate) const fn base(&self) -> u64 {
        self.base
    }

    pub(crate) const fn image(&self) -> &'i Image<'a> {
        self.image
    }

    /// The begin RVA of the function whose code holds `rva`.
    pub(crate) fn function_of(&self, rva: u64) -> Option<u32> {
        let rva = u32::try_from(rva).ok()?;
        let i = self
            .parts
            .partition_point(|&(begin, _)| begin <= rva)
            .checked_sub(1)?;
        (rva < self.parts[i].1).then(|| self.primary[i])
    }

    /// The parts of the function that begins at `begin`: its own first, then
    /// its fragments by address.
    pub(crate) fn parts_of(&self, begin: u32) -> impl Iterator<Item = (u32, u32)> + '_ {
        let from = self.by_function.partition_point(|&(f, ..)| f < begin);
        self.by_function[from..]
            .iter()
            .take_while(move |&&(f, ..)| f == begin)
            .map(|&(_, b, e)| (b, e))
    }

    /// The instructions of one part, in order; empty when it does not read.
    pub(crate) fn decode(&self, (begin, end): (u32, u32)) -> Vec<Instruction> {
        let va = self.base + u64::from(begin);
        let Some(bytes) = self.image.read(va, u64::from(end - begin)) else {
            return Vec::new();
        };
        let mut decoder = Decoder::with_ip(64, bytes, va, DecoderOptions::NONE);
        let mut out = Vec::with_capacity(bytes.len() / 4);
        while decoder.can_decode() {
            out.push(decoder.decode());
        }
        out
    }

    /// The RVAs every part of the function at `begin` calls directly.
    pub(crate) fn callees(&self, begin: u32) -> FxHashSet<u64> {
        self.parts_of(begin)
            .flat_map(|part| self.decode(part))
            .filter(|i| i.mnemonic() == Mnemonic::Call && i.op0_kind() == OpKind::NearBranch64)
            .filter_map(|i| i.near_branch64().checked_sub(self.base))
            .collect()
    }

    /// For each of `targets`, the begin RVAs of the functions whose code
    /// loads its address with a RIP-relative `lea`, from one pass over the
    /// code section.
    pub(crate) fn loaders(&self, targets: &[u64]) -> Vec<FxHashSet<u32>> {
        let mut found = vec![FxHashSet::default(); targets.len()];
        let Some(code) = self
            .image
            .read(self.text.start, self.text.end - self.text.start)
        else {
            return found;
        };
        for (at, window) in code.windows(6).enumerate() {
            if window[0] != LEA || window[1] & MODRM_RIP.0 != MODRM_RIP.1 {
                continue;
            }
            let disp = i32::from_le_bytes([window[2], window[3], window[4], window[5]]);
            let next = self.text.start + at as u64 + 6;
            let Some(target) = next.checked_add_signed(i64::from(disp)) else {
                continue;
            };
            let Some(t) = targets.iter().position(|&want| want == target) else {
                continue;
            };
            // The opcode may sit inside another instruction: only a decode
            // from its REX prefix, or from the opcode, that ends at the
            // displacement counts.
            let confirmed = [1u64, 0].into_iter().any(|rex| {
                let start = self.text.start + at as u64 - rex;
                at as u64 >= rex
                    && self.image.read(start, 6 + rex).is_some_and(|bytes| {
                        let i = Decoder::with_ip(64, bytes, start, DecoderOptions::NONE).decode();
                        i.mnemonic() == Mnemonic::Lea
                            && i.next_ip() == next
                            && i.ip_rel_memory_address() == target
                    })
            });
            if confirmed && let Some(function) = self.function_of(next - self.base) {
                found[t].insert(function);
            }
        }
        found
    }

    /// The `.rdata` addresses where `text` lies as a whole NUL-terminated
    /// string.
    pub(crate) fn strings(&self, text: &str) -> Vec<u64> {
        let Some(rdata) = self.image.section(".rdata").map(|s| self.image.range(s)) else {
            return Vec::new();
        };
        let Some(bytes) = self.image.read(rdata.start, rdata.end - rdata.start) else {
            return Vec::new();
        };
        let needle: Vec<u8> = text.bytes().chain([0]).collect();
        bytes
            .windows(needle.len())
            .enumerate()
            .filter(|&(at, window)| window == needle && (at == 0 || bytes[at - 1] == 0))
            .map(|(at, _)| rdata.start + at as u64)
            .collect()
    }
}

/// The begin RVA of the function a part at `begin` with unwind info at
/// `unwind` belongs to, following chained unwind info.
fn primary_of(image: &Image<'_>, base: u64, mut begin: u32, mut unwind: u32) -> u32 {
    for _ in 0..MAX_CHAIN {
        let Some(head) = image.read(base + u64::from(unwind), 4) else {
            return begin;
        };
        if (head[0] >> 3) & CHAIN_INFO == 0 {
            return begin;
        }
        let codes = (u64::from(head[2]) + 1) & !1;
        let parent = base + u64::from(unwind) + 4 + codes * 2;
        match (image.u32(parent), image.u32(parent + 8)) {
            (Some(b), Some(u)) => (begin, unwind) = (b, u),
            _ => return begin,
        }
    }
    begin
}
