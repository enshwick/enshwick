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
/// The padding MSVC puts between functions.
const INT3: u8 = 0xcc;
/// The most code a function outside `.pdata` is read for.
const MAX_LEAF: u32 = 0x4000;
/// Parts decoded per task of the first pass, so the cores share the work.
const PARTS_PER_TASK: usize = 2048;
/// Pieces the string pass over `.rdata` is cut into, so the cores share it.
const STRING_PIECES: usize = 64;

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
    /// Every RIP-relative `lea` of a wanted address as `(address,
    /// function)`, sorted.
    loads: Vec<(u64, u32)>,
}

/// What the decode of one part found.
#[derive(Default)]
struct Branches {
    /// `(caller function, callee)`.
    calls: Vec<(u32, u32)>,
    /// `(target, jumping function)` of jumps that leave their function.
    jumps: Vec<(u32, u32)>,
    /// `(loaded address, loading function)`.
    loads: Vec<(u64, u32)>,
}

impl<'i, 'a> Code<'i, 'a> {
    /// The functions `image` declares and the code outside them, with the
    /// functions that load each address of `wanted` with a RIP-relative
    /// `lea`; `None` without `.text` or `.pdata`.
    pub(crate) fn new(image: &'i Image<'a>, wanted: &FxHashSet<u64>) -> Option<Self> {
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
            loads: Vec::new(),
        };
        let found = crate::table::search(&tasks(code.parts.len()), |range| {
            Some(code.branches(range.clone(), wanted))
        });
        let mut branches = Branches::default();
        for part in found.into_iter().flatten() {
            branches.calls.extend(part.calls);
            branches.jumps.extend(part.jumps);
            branches.loads.extend(part.loads);
        }
        code.add_outside(&mut branches, wanted);
        code.index(&branches.calls);
        branches.loads.sort_unstable();
        branches.loads.dedup();
        code.loads = branches.loads;
        Some(code)
    }

    /// The direct calls, the jumps out of their function and the loads of
    /// `wanted` addresses in the parts `range` of the exception directory.
    fn branches(&self, range: Range<usize>, wanted: &FxHashSet<u64>) -> Branches {
        let mut out = Branches::default();
        for i in range {
            let function = self.primary[i];
            for ins in self.instructions(self.parts[i]) {
                if ins.mnemonic() == Mnemonic::Lea
                    && ins.is_ip_rel_memory_operand()
                    && wanted.contains(&ins.ip_rel_memory_address())
                {
                    out.loads.push((ins.ip_rel_memory_address(), function));
                }
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
    fn add_outside(&mut self, branches: &mut Branches, wanted: &FxHashSet<u64>) {
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
        let caps: Vec<(u32, u32)> = leaves
            .iter()
            .enumerate()
            .map(|(at, &leaf)| {
                let next = self.parts.partition_point(|&(begin, _)| begin <= leaf);
                let limit = self.parts.get(next).map_or(text_end, |&(begin, _)| begin);
                let cap = limit.saturating_sub(leaf).min(MAX_LEAF);
                let cap = leaves
                    .get(at + 1)
                    .map_or(cap, |&after| cap.min(after - leaf));
                (leaf, cap)
            })
            .collect();
        let ends = crate::table::search(&tasks(caps.len()), |range| {
            Some(
                caps[range.clone()]
                    .iter()
                    .map(|&(leaf, cap)| (leaf, self.run_end(leaf, cap)))
                    .collect::<Vec<_>>(),
            )
        });
        let mut added: Vec<(u32, u32)> = Vec::new();
        for (leaf, end) in ends.into_iter().flatten().flatten() {
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
            for ins in self.instructions(part) {
                if ins.mnemonic() == Mnemonic::Lea
                    && ins.is_ip_rel_memory_operand()
                    && wanted.contains(&ins.ip_rel_memory_address())
                {
                    branches.loads.push((ins.ip_rel_memory_address(), owner));
                }
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
        let (Some(first), Some(last)) = (
            rva(self.text.start, self.base),
            rva(self.text.end, self.base),
        ) else {
            return (Vec::new(), FxHashMap::default());
        };
        // Each gap as `(begin, end, function whose part ends at its begin)`.
        let mut gaps: Vec<(u32, u32, Option<u32>)> = Vec::new();
        let mut prev = first;
        let mut owner: Option<u32> = None;
        let ends = self
            .parts
            .iter()
            .zip(self.primary.iter().map(|&f| Some(f)))
            .map(|(&part, f)| (part, f))
            .chain([((last, 0), None)]);
        for ((begin, end), function) in ends {
            if prev < begin {
                gaps.push((prev, begin, owner));
            }
            if end >= prev {
                prev = end;
                owner = function;
            }
        }
        let found = crate::table::search(&tasks(gaps.len()), |range| {
            let mut starts = Vec::new();
            let mut tails = Vec::new();
            for &(from, to, owner) in &gaps[range.clone()] {
                let mut at = from;
                while at < to {
                    let Some(bytes) = self
                        .image
                        .read(self.base + u64::from(at), u64::from(to - at))
                    else {
                        break;
                    };
                    let pad = bytes.iter().position(|&b| b != INT3).unwrap_or(bytes.len());
                    let pad = u32::try_from(pad).unwrap_or(u32::MAX);
                    if pad == 0
                        && at == from
                        && let Some(owner) = owner
                    {
                        tails.push((at, owner));
                    }
                    at = at.saturating_add(pad);
                    if at >= to {
                        break;
                    }
                    starts.push(at);
                    at = self.run_end(at, to - at).max(at + 1);
                }
            }
            Some((starts, tails))
        });
        let mut starts = Vec::new();
        let mut tails = FxHashMap::default();
        for (s, t) in found.into_iter().flatten() {
            starts.extend(s);
            tails.extend(t);
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

    /// The begin RVAs of the functions whose code loads the wanted address
    /// `va` with a RIP-relative `lea`.
    pub(crate) fn loaders(&self, va: u64) -> impl Iterator<Item = u32> + '_ {
        let from = self.loads.partition_point(|&(at, _)| at < va);
        self.loads[from..]
            .iter()
            .take_while(move |&&(at, _)| at == va)
            .map(|&(_, function)| function)
    }

    /// The instructions of one part, in order, decoded as they are read.
    fn instructions(&self, (begin, end): (u32, u32)) -> impl Iterator<Item = Instruction> + 'a {
        let va = self.base + u64::from(begin);
        let bytes = self
            .image
            .read(va, u64::from(end - begin))
            .unwrap_or_default();
        Decoder::with_ip(64, bytes, va, DecoderOptions::NONE).into_iter()
    }
}

/// For each of `texts`, the `.rdata` addresses where it lies as a whole
/// NUL-terminated string, from one pass over the strings there on all cores.
pub(crate) fn strings(image: &Image<'_>, texts: &[&str]) -> Vec<Vec<u64>> {
    let mut found = vec![Vec::new(); texts.len()];
    let Some(rdata) = image.section(".rdata").map(|s| image.range(s)) else {
        return found;
    };
    let Some(bytes) = image.read(rdata.start, rdata.end - rdata.start) else {
        return found;
    };
    let mut by_text: FxHashMap<&[u8], Vec<usize>> = FxHashMap::default();
    // Only a string of a wanted length is looked up.
    let mut wanted_length = vec![false; texts.iter().map(|t| t.len() + 1).max().unwrap_or(0)];
    for (t, text) in texts.iter().enumerate() {
        by_text.entry(text.as_bytes()).or_default().push(t);
        wanted_length[text.len()] = true;
    }
    // Pieces of `.rdata` that each start right after a NUL, or at its start.
    let mut cuts: Vec<usize> = (0..STRING_PIECES)
        .map(|i| bytes.len() * i / STRING_PIECES)
        .map(|at| match at {
            0 => 0,
            at => memchr::memchr(0, &bytes[at..]).map_or(bytes.len(), |nul| at + nul + 1),
        })
        .chain([bytes.len()])
        .collect();
    cuts.dedup();
    let pieces: Vec<Range<usize>> = cuts.windows(2).map(|w| w[0]..w[1]).collect();
    let hits = crate::table::search(&pieces, |piece| {
        let mut hits = Vec::new();
        let mut at = piece.start;
        for end in memchr::memchr_iter(0, &bytes[piece.clone()]).map(|nul| piece.start + nul) {
            if wanted_length.get(end - at).copied().unwrap_or(false)
                && let Some(ts) = by_text.get(&bytes[at..end])
            {
                hits.extend(ts.iter().map(|&t| (t, rdata.start + at as u64)));
            }
            at = end + 1;
        }
        Some(hits)
    });
    for (t, va) in hits.into_iter().flatten().flatten() {
        found[t].push(va);
    }
    found
}

/// `0..len` cut into ranges of [`PARTS_PER_TASK`], one task each.
fn tasks(len: usize) -> Vec<Range<usize>> {
    (0..len)
        .step_by(PARTS_PER_TASK)
        .map(|at| at..(at + PARTS_PER_TASK).min(len))
        .collect()
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
