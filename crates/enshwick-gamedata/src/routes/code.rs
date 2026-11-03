//! The game's code as functions: their extents from the exception directory
//! (`.pdata`), each with the fragments the compiler split off it, and the
//! code outside it (leaf functions and function tails), with every direct
//! call between them, from one decode of the whole code on all cores.

use core::ops::Range;

use iced_x86::{Decoder, DecoderOptions, Instruction, Mnemonic, OpKind};
use rustc_hash::{FxHashMap, FxHashSet};

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
/// The padding MSVC puts between functions.
const INT3: u8 = 0xcc;
/// The most code a function outside `.pdata` is read for.
const MAX_LEAF: u32 = 0x4000;
/// Parts decoded per task of the first pass, so the cores share the work.
const PARTS_PER_TASK: usize = 2048;

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
    /// Every direct call as `caller function << 32 | callee`, sorted.
    calls: Vec<u64>,
    /// Every direct call as `callee << 32 | caller function`, sorted.
    called: Vec<u64>,
}

/// What the decode of one part found.
#[derive(Default)]
struct Branches {
    /// `(caller function, callee)`.
    calls: Vec<(u32, u32)>,
    /// `(target, jumping function)` of jumps that leave their function.
    jumps: Vec<(u32, u32)>,
}

impl<'i, 'a> Code<'i, 'a> {
    /// The functions `image` declares and the code outside them; `None`
    /// without `.text` or `.pdata`.
    pub(crate) fn new(image: &'i Image<'a>) -> Option<Self> {
        let base = image.base();
        let text = image.range(image.section(".text")?);
        let pdata = image.range(image.section(".pdata")?);
        let table = image.read(pdata.start, pdata.end - pdata.start)?;
        let mut raw: Vec<(u32, u32, u32)> = table
            .as_chunks::<RUNTIME_FUNCTION>()
            .0
            .iter()
            .map(|f| {
                let word = |at: usize| u32::from_le_bytes([f[at], f[at + 1], f[at + 2], f[at + 3]]);
                (word(0), word(4), word(8))
            })
            .filter(|&(begin, end, _)| begin != 0 && begin < end)
            .collect();
        raw.sort_unstable();
        let mut code = Self {
            image,
            base,
            text,
            primary: raw
                .iter()
                .map(|&(begin, _, unwind)| primary_of(image, base, begin, unwind))
                .collect(),
            parts: raw.into_iter().map(|(b, e, _)| (b, e)).collect(),
            by_function: Vec::new(),
            calls: Vec::new(),
            called: Vec::new(),
        };
        let tasks: Vec<Range<usize>> = (0..code.parts.len())
            .step_by(PARTS_PER_TASK)
            .map(|at| at..(at + PARTS_PER_TASK).min(code.parts.len()))
            .collect();
        let found = crate::table::search(&tasks, |range| Some(code.branches(range.clone())));
        let mut branches = Branches::default();
        for part in found.into_iter().flatten() {
            branches.calls.extend(part.calls);
            branches.jumps.extend(part.jumps);
        }
        code.add_outside(&mut branches);
        code.index(&branches.calls);
        Some(code)
    }

    /// The direct calls and the jumps out of their function in the parts
    /// `range` of the exception directory.
    fn branches(&self, range: Range<usize>) -> Branches {
        let mut out = Branches::default();
        for i in range {
            let function = self.primary[i];
            for ins in self.decode(self.parts[i]) {
                if ins.op0_kind() != OpKind::NearBranch64 {
                    continue;
                }
                let Some(target) = rva(ins.near_branch64(), self.base) else {
                    continue;
                };
                match ins.mnemonic() {
                    Mnemonic::Call => out.calls.push((function, target)),
                    Mnemonic::Jmp if self.function_of(u64::from(target)) != Some(function) => {
                        out.jumps.push((target, function));
                    }
                    _ => {}
                }
            }
        }
        out
    }

    /// Adds the code no `.pdata` entry covers: each run of code a call or a
    /// jump out of a function reaches, or that follows the padding after a
    /// function, up to the first `int3`. A run right after a function's
    /// part with no padding between is the rest of that function, unless
    /// another function calls or jumps to it.
    fn add_outside(&mut self, branches: &mut Branches) {
        let (starts, tails) = self.gaps();
        let text_end = rva(self.text.end, self.base).unwrap_or(0);
        let called: FxHashSet<u32> = branches.calls.iter().map(|&(_, t)| t).collect();
        let mut jumpers: FxHashMap<u32, Vec<u32>> = FxHashMap::default();
        for &(target, from) in &branches.jumps {
            jumpers.entry(target).or_default().push(from);
        }
        let mut leaves: Vec<u32> = called
            .iter()
            .copied()
            .chain(jumpers.keys().copied())
            .chain(starts)
            .filter(|&t| self.text.contains(&(self.base + u64::from(t))))
            .filter(|&t| self.function_of(u64::from(t)).is_none())
            .collect();
        leaves.sort_unstable();
        leaves.dedup();
        let mut added: Vec<(u32, u32)> = Vec::new();
        for (at, &leaf) in leaves.iter().enumerate() {
            let next = self.parts.partition_point(|&(begin, _)| begin <= leaf);
            let limit = self.parts.get(next).map_or(text_end, |&(begin, _)| begin);
            let mut cap = limit.saturating_sub(leaf).min(MAX_LEAF);
            if let Some(&after) = leaves.get(at + 1) {
                cap = cap.min(after - leaf);
            }
            let end = self.run_end(leaf, cap);
            if end > leaf && added.last().is_none_or(|&(_, e)| e <= leaf) {
                added.push((leaf, end));
            }
        }
        let owners: Vec<u32> = added
            .iter()
            .map(|&(begin, _)| {
                tails
                    .get(&begin)
                    .copied()
                    .filter(|&owner| {
                        !called.contains(&begin)
                            && jumpers
                                .get(&begin)
                                .is_none_or(|from| from.iter().all(|&f| f == owner))
                    })
                    .unwrap_or(begin)
            })
            .collect();
        for (&part, &owner) in added.iter().zip(&owners) {
            for ins in self.decode(part) {
                if ins.mnemonic() == Mnemonic::Call
                    && ins.op0_kind() == OpKind::NearBranch64
                    && let Some(target) = rva(ins.near_branch64(), self.base)
                {
                    branches.calls.push((owner, target));
                }
            }
        }
        let mut all: Vec<((u32, u32), u32)> = self
            .parts
            .iter()
            .copied()
            .zip(self.primary.iter().copied())
            .chain(added.into_iter().zip(owners))
            .collect();
        all.sort_unstable();
        self.parts = all.iter().map(|&(part, _)| part).collect();
        self.primary = all.iter().map(|&(_, function)| function).collect();
    }

    /// Where each run of code between the parts of `.pdata` starts, and the
    /// runs that start right at a part's end, by the function owning it.
    fn gaps(&self) -> (Vec<u32>, FxHashMap<u32, u32>) {
        let mut starts = Vec::new();
        let mut tails = FxHashMap::default();
        let (Some(first), Some(last)) = (
            rva(self.text.start, self.base),
            rva(self.text.end, self.base),
        ) else {
            return (starts, tails);
        };
        let mut prev = first;
        let mut owner: Option<u32> = None;
        let ends = self
            .parts
            .iter()
            .zip(self.primary.iter().map(|&f| Some(f)))
            .map(|(&part, f)| (part, f))
            .chain([((last, 0), None)]);
        for ((begin, end), function) in ends {
            let mut at = prev;
            while at < begin {
                let Some(bytes) = self
                    .image
                    .read(self.base + u64::from(at), u64::from(begin - at))
                else {
                    break;
                };
                let pad = bytes.iter().position(|&b| b != INT3).unwrap_or(bytes.len());
                let pad = u32::try_from(pad).unwrap_or(u32::MAX);
                if pad == 0
                    && at == prev
                    && let Some(owner) = owner
                {
                    tails.insert(at, owner);
                }
                at = at.saturating_add(pad);
                if at >= begin {
                    break;
                }
                starts.push(at);
                at = self.run_end(at, begin - at).max(at + 1);
            }
            if end >= prev {
                prev = end;
                owner = function;
            }
        }
        (starts, tails)
    }

    /// The end RVA of the run of code at `begin`, read for at most `cap`
    /// bytes: the end of the last instruction before an `int3` or bytes that
    /// do not decode.
    fn run_end(&self, begin: u32, cap: u32) -> u32 {
        let va = self.base + u64::from(begin);
        let Some(bytes) = self.image.read(va, u64::from(cap)) else {
            return begin;
        };
        let mut decoder = Decoder::with_ip(64, bytes, va, DecoderOptions::NONE);
        let mut end = va;
        while decoder.can_decode() {
            let ins = decoder.decode();
            if ins.is_invalid() || ins.mnemonic() == Mnemonic::Int3 {
                break;
            }
            end = ins.next_ip();
        }
        rva(end, self.base).unwrap_or(begin)
    }

    /// Builds the part and call indexes from the parts and `calls`.
    fn index(&mut self, calls: &[(u32, u32)]) {
        let mut by_function: Vec<(u32, u32, u32)> = self
            .parts
            .iter()
            .zip(&self.primary)
            .map(|(&(b, e), &function)| (function, b, e))
            .collect();
        // A function's own part sorts first: its begin is the function's.
        by_function.sort_unstable_by_key(|&(f, b, _)| (f, b != f, b));
        self.by_function = by_function;
        let pack = |high: u32, low: u32| u64::from(high) << 32 | u64::from(low);
        self.calls = calls.iter().map(|&(from, to)| pack(from, to)).collect();
        self.called = calls.iter().map(|&(from, to)| pack(to, from)).collect();
        for list in [&mut self.calls, &mut self.called] {
            list.sort_unstable();
            list.dedup();
        }
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
    pub(crate) fn callees(&self, begin: u32) -> FxHashSet<u32> {
        low_halves(&self.calls, begin).collect()
    }

    /// The begin RVAs of the functions that call `rva` directly.
    pub(crate) fn callers(&self, rva: u32) -> FxHashSet<u32> {
        low_halves(&self.called, rva).collect()
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
        for at in memchr::memchr_iter(LEA, code) {
            let Some(window) = code.get(at..at + 6) else {
                break;
            };
            if window[1] & MODRM_RIP.0 != MODRM_RIP.1 {
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

    /// For each of `texts`, the `.rdata` addresses where it lies as a whole
    /// NUL-terminated string, from one pass over the strings there.
    pub(crate) fn strings(&self, texts: &[&str]) -> Vec<Vec<u64>> {
        let mut found = vec![Vec::new(); texts.len()];
        let Some(rdata) = self.image.section(".rdata").map(|s| self.image.range(s)) else {
            return found;
        };
        let Some(bytes) = self.image.read(rdata.start, rdata.end - rdata.start) else {
            return found;
        };
        let mut at = 0;
        for end in memchr::memchr_iter(0, bytes) {
            let string = &bytes[at..end];
            if !string.is_empty()
                && let Some(t) = texts.iter().position(|t| t.as_bytes() == string)
            {
                found[t].push(rdata.start + at as u64);
            }
            at = end + 1;
        }
        found
    }
}

/// The RVA of `va` in an image at `base`.
fn rva(va: u64, base: u64) -> Option<u32> {
    u32::try_from(va.checked_sub(base)?).ok()
}

/// The low halves of the entries of the sorted `packed` whose high half is
/// `high`.
fn low_halves(packed: &[u64], high: u32) -> impl Iterator<Item = u32> + '_ {
    let from = packed.partition_point(|&p| (p >> 32) < u64::from(high));
    packed[from..]
        .iter()
        .take_while(move |&&p| (p >> 32) == u64::from(high))
        .map(|&p| p.to_le_bytes())
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
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
