//! Arming: one detour per target, through a loader-owned thunk.
//!
//! Each target is detoured once, to a thunk that jumps through the chain's head
//! slot. Thunks sit on pages that are made execute-only once written; the slots
//! live on separate read-write pages, so rewiring never writes code. A thunk
//! for a chain that reads the caller frame first stores the stack pointer in a
//! TLS slot of the thread.
//!
//! Mid-function hooks are armed in the same batch: their stubs come from
//! [`crate::mid`], their jumps from the same detour engine.
//!
//! With diagnostics on, each thunk also counts its calls in a counter beside
//! its slot, one locked increment. With timing, every 256th call goes the
//! long way: the thunk gives the chain a frame of its own, with the caller's
//! stack arguments copied into it and unwind data registered for it, and
//! adds the time-stamp counter ticks the call took to the counter. Timing
//! also puts a timed thunk in front of every link and every original, which
//! a link's `next` slot points at, so each link's own time is its thunk's
//! less the next one's. Without diagnostics the thunks are the plain ones
//! and count nothing.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};

use windows_sys::Win32::System::Diagnostics::Debug::{
    FlushInstructionCache, IMAGE_RUNTIME_FUNCTION_ENTRY, RtlAddFunctionTable,
};
use windows_sys::Win32::System::Memory::{
    MEM_COMMIT, MEM_RESERVE, PAGE_EXECUTE_READ, PAGE_READWRITE, VirtualAlloc, VirtualProtect,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, TlsAlloc, TlsGetValue};

pub(crate) use crate::chains::Counter;
use crate::chains::{Chains, Hop};
use crate::code::{CodePatch, Verdict};
use crate::detour::{Pending, Request};
use crate::mid::{Mids, Stubs};

const PAGE: usize = 4096;
const THUNK: usize = 96;
/// A thunk that times every 256th call.
const TIMED_THUNK: usize = 512;
/// Stack arguments a timed call copies: the fifth to the sixteenth.
const STACK_ARGS: u32 = 12;
/// The timed call's frame: home space, the copied arguments, the start time
/// and the saved result, keeping the call 16-byte aligned.
const FRAME: u32 = 0x20 + STACK_ARGS * 8 + 16 + 8;
const START: u32 = 0x20 + STACK_ARGS * 8;
const SAVED_RESULT: u32 = START + 8;
/// `sub rsp, imm32`, the timed call's prolog.
const PROLOG: u8 = 7;
/// Unwind data of the timed call: version 1, the prolog, two codes for
/// `UWOP_ALLOC_LARGE` with the frame size in 8-byte units.
#[expect(clippy::cast_possible_truncation, reason = "the frame is under 2 KiB")]
const UNWIND: [u8; 8] = [0x01, PROLOG, 2, 0, PROLOG, 0x01, (FRAME / 8) as u8, 0];
/// `TEB.TlsSlots` on x64; the first 64 TLS indices live there.
const TEB_TLS_SLOTS: u32 = 0x1480;
const TLS_INLINE_SLOTS: u32 = 64;
const TLS_OUT_OF_INDEXES: u32 = u32::MAX;

/// The registers a frame thunk saves, in `CallerFrame` order, with the REX
/// prefix and `ModRM` byte of `mov gs:[disp32], reg`.
const SAVED: [(u8, u8); 9] = [
    (0x48, 0x24), // rsp
    (0x48, 0x1c), // rbx
    (0x48, 0x2c), // rbp
    (0x48, 0x3c), // rdi
    (0x48, 0x34), // rsi
    (0x4c, 0x24), // r12
    (0x4c, 0x2c), // r13
    (0x4c, 0x34), // r14
    (0x4c, 0x3c), // r15
];

/// The TLS indices that hold the caller frame, if they could be had.
#[derive(Clone, Copy, Debug)]
pub(crate) struct FrameSlot([u32; 9]);

impl FrameSlot {
    /// Allocates the indices; `None` when not all of them are ones the thunk
    /// can address.
    #[must_use]
    pub(crate) fn allocate() -> Option<Self> {
        let mut indices = [0u32; 9];
        for index in &mut indices {
            // SAFETY: no preconditions.
            let allocated = unsafe { TlsAlloc() };
            if allocated == TLS_OUT_OF_INDEXES || allocated >= TLS_INLINE_SLOTS {
                return None;
            }
            *index = allocated;
        }
        Some(Self(indices))
    }

    /// The caller frame the thunk stored on this thread.
    #[must_use]
    pub(crate) fn read(self) -> enshwick_loader_api::abi::CallerFrame {
        // SAFETY: each index was allocated by `allocate`.
        let value = |i: usize| unsafe { TlsGetValue(self.0[i]) }.addr() as u64;
        enshwick_loader_api::abi::CallerFrame {
            rsp: value(0),
            rbx: value(1),
            rbp: value(2),
            rdi: value(3),
            rsi: value(4),
            r12: value(5),
            r13: value(6),
            r14: value(7),
            r15: value(8),
        }
    }
}

/// What thunks count.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Count {
    /// Nothing: the plain thunk.
    Off,
    /// Calls.
    Calls,
    /// Calls, and the time of every 256th.
    Timed,
}

impl Count {
    /// What the settings read at start ask for.
    pub(crate) fn from_settings() -> Self {
        if crate::diag::timing() {
            Self::Timed
        } else if crate::diag::counting() {
            Self::Calls
        } else {
            Self::Off
        }
    }

    /// How many timed thunks the links and originals of `chains` take.
    fn hops(self, chains: &Chains) -> usize {
        if self == Self::Timed {
            chains.iter().map(|c| c.links.len() + 1).sum()
        } else {
            0
        }
    }

    const fn thunk(self) -> usize {
        match self {
            Self::Off | Self::Calls => THUNK,
            Self::Timed => TIMED_THUNK,
        }
    }
}

/// Each armed function's counters, by target, once armed with counting.
static COUNTERS: OnceLock<Box<[(u64, &'static Counter)]>> = OnceLock::new();
/// The time-stamp counter and the clock when the hooks were armed, to measure
/// the counter's rate.
static ARMED_AT: OnceLock<(u64, std::time::Instant)> = OnceLock::new();

/// Each armed function's counters, by target; empty without counting.
pub(crate) fn counters() -> &'static [(u64, &'static Counter)] {
    COUNTERS.get().map_or(&[], |c| c)
}

/// The time-stamp counter's rate in ticks a second, measured since arming;
/// `None` before a second has passed.
#[expect(clippy::cast_precision_loss, reason = "a rate for people to read")]
pub(crate) fn tsc_hz() -> Option<f64> {
    let &(ticks, at) = ARMED_AT.get()?;
    let seconds = at.elapsed().as_secs_f64();
    let now = rdtsc();
    (seconds >= 1.0).then(|| now.wrapping_sub(ticks) as f64 / seconds)
}

fn rdtsc() -> u64 {
    // SAFETY: `rdtsc` has no preconditions on x86-64.
    unsafe { core::arch::x86_64::_rdtsc() }
}

/// A target that could not be armed, and why.
#[derive(Debug)]
pub(crate) struct Failure {
    /// The game function, or the instruction of a mid-function hook.
    pub(crate) target: u64,
    /// A mid-function hook's address rather than a chain's function.
    pub(crate) inside: bool,
    /// For the log.
    pub(crate) reason: String,
}

impl Failure {
    fn chain(target: u64, reason: impl Into<String>) -> Self {
        Self {
            target,
            inside: false,
            reason: reason.into(),
        }
    }

    fn site(target: u64, reason: impl Into<String>) -> Self {
        Self {
            target,
            inside: true,
            reason: reason.into(),
        }
    }
}

/// What arming did.
#[derive(Debug, Default)]
pub(crate) struct Armed {
    /// The targets that failed; their chains stay unarmed and the game calls
    /// those functions unchanged.
    pub(crate) failures: Vec<Failure>,
    /// What became of each code patch, in order.
    pub(crate) code: Vec<Verdict>,
}

/// Detours every chain's target to its thunk, wires the slots, jumps from
/// every mid-function hook's address to its stub and writes the code
/// patches, all in one batch.
pub(crate) fn arm(
    chains: &mut Chains,
    mids: &Mids,
    frame: Option<FrameSlot>,
    off: fn(usize) -> bool,
    patches: &[CodePatch],
    counting: Count,
) -> Armed {
    let count = chains.len();
    if count == 0 {
        let mut armed = Armed::default();
        armed.code = hook(chains, mids, &[], off, patches, &mut armed.failures);
        return armed;
    }
    let thunk = counting.thunk();
    let hops = counting.hops(chains);
    let code_len = (count * thunk + hops * TIMED_THUNK).div_ceil(PAGE) * PAGE;
    let slots_len = (count + hops) * size_of::<usize>();
    let counters_len = (count + hops) * size_of::<Counter>();
    let functions_len = (count + hops) * size_of::<RuntimeFunction>();
    let data_len = (slots_len + counters_len + functions_len + UNWIND.len()).div_ceil(PAGE) * PAGE;
    let block = reserve(code_len + data_len);
    if block.is_null() {
        return unarmed(chains, mids, patches, "no memory for thunks");
    }
    // SAFETY: `block` spans `code_len + data_len` committed bytes.
    let data_start = unsafe { block.add(code_len) };
    #[expect(
        clippy::cast_ptr_alignment,
        reason = "the data pages start on a page boundary"
    )]
    let data = data_start.cast::<AtomicUsize>();
    // SAFETY: inside the data pages, after the slots.
    let counter_start = unsafe { data_start.add(slots_len) };
    #[expect(
        clippy::cast_ptr_alignment,
        reason = "the slots end on an 8-byte boundary"
    )]
    let counter_base = counter_start.cast::<Counter>();
    // SAFETY: inside the data pages, after the counters.
    let function_start = unsafe { data_start.add(slots_len + counters_len) };
    #[expect(
        clippy::cast_ptr_alignment,
        reason = "the counters end on an 8-byte boundary"
    )]
    let function_base = function_start.cast::<RuntimeFunction>();
    // SAFETY: inside the data pages, after the function table.
    let unwind = unsafe { data_start.add(slots_len + counters_len + functions_len) };
    // SAFETY: `UNWIND.len()` bytes of the data pages are left for it.
    unsafe { unwind.copy_from_nonoverlapping(UNWIND.as_ptr(), UNWIND.len()) };

    let mut failures = Vec::new();
    let mut thunks = Vec::with_capacity(count);
    let mut counters = Vec::new();
    let mut table = Table {
        base: block,
        entries: function_base,
        unwind,
        count: 0,
    };
    for (i, chain) in chains.iter_mut().enumerate() {
        // SAFETY: `i < count`, inside the data pages.
        let slot_ptr = unsafe { data.add(i) };
        // SAFETY: committed, zeroed and aligned memory that is never freed.
        let slot: &'static AtomicUsize = unsafe { &*slot_ptr };
        // SAFETY: `i < count`, inside the code pages.
        let code = unsafe { block.add(i * thunk) };
        let frame_index = match (chain.wants_caller_frame(), frame) {
            (false, _) => None,
            (true, Some(frame)) => Some(frame.0),
            (true, None) => {
                failures.push(Failure::chain(
                    chain.target,
                    "no TLS slot for the caller frame",
                ));
                continue;
            }
        };
        let counter = (counting != Count::Off).then(|| {
            // SAFETY: `i < count`, inside the counter area.
            let counter_ptr = unsafe { counter_base.add(i) };
            // SAFETY: committed, zeroed and aligned memory that is never
            // freed; zero is a valid counter.
            let counter: &'static Counter = unsafe { &*counter_ptr };
            counters.push((chain.target, counter));
            (counter, counting == Count::Timed)
        });
        // SAFETY: `code` has `thunk` writable bytes; the slot and the counter
        // are within ±2 GiB.
        let written = unsafe { write_thunk(code, thunk, slot, frame_index, counter) };
        // SAFETY: one entry per thunk fits the table, all inside `block`.
        unsafe { table.add(code, &written) };
        chain.head = Some(slot);
        thunks.push((chain.target, code));
    }
    if hops > 0 {
        let hop_code = Hops {
            // SAFETY: the hops' thunks follow the heads', inside the code pages.
            code: unsafe { block.add(count * thunk) },
            // SAFETY: the hops' slots follow the heads', inside the data pages.
            slots: unsafe { data.add(count) },
            // SAFETY: as above, for the counters.
            counters: unsafe { counter_base.add(count) },
            count: hops,
        };
        // SAFETY: the hop areas hold `hops` entries each, never freed.
        unsafe { hop_code.arm(chains, &mut table) };
    }
    // SAFETY: the entries written above, in memory never freed.
    unsafe { register_unwind(block, function_base, table.count) };
    if counting != Count::Off {
        let _first = COUNTERS.set(counters.into_boxed_slice());
        let _first = ARMED_AT.set((rdtsc(), std::time::Instant::now()));
    }

    let mut old = 0;
    // SAFETY: the code pages belong to `block`.
    let sealed = unsafe { VirtualProtect(block.cast(), code_len, PAGE_EXECUTE_READ, &raw mut old) };
    if sealed == 0 {
        return unarmed(chains, mids, patches, "thunk pages not executable");
    }
    // SAFETY: no preconditions.
    let process = unsafe { GetCurrentProcess() };
    // SAFETY: flushing our own process's freshly written code.
    unsafe { FlushInstructionCache(process, block.cast(), code_len) };

    let code = hook(chains, mids, &thunks, off, patches, &mut failures);
    Armed { failures, code }
}

/// `len` bytes of fresh read-write memory; null when there is none.
fn reserve(len: usize) -> *mut u8 {
    // SAFETY: a fresh reservation; null on failure.
    unsafe {
        VirtualAlloc(
            core::ptr::null(),
            len,
            MEM_RESERVE | MEM_COMMIT,
            PAGE_READWRITE,
        )
    }
    .cast::<u8>()
}

/// The function table of the timed calls, filled as thunks are written.
struct Table {
    base: *mut u8,
    entries: *mut RuntimeFunction,
    unwind: *mut u8,
    count: u32,
}

impl Table {
    /// Adds the timed call of the thunk at `code`, if it has one.
    ///
    /// # Safety
    ///
    /// The table has room for one more entry, and `code` and the unwind data
    /// lie inside the block at `base`.
    unsafe fn add(&mut self, code: *mut u8, written: &Written) {
        let Some((timed, end)) = written.timed else {
            return;
        };
        let offset = |at: usize| u32::try_from(at - self.base.addr()).unwrap_or(0);
        // SAFETY: the caller guarantees room for this entry.
        let entry = unsafe { self.entries.add(self.count as usize) };
        // SAFETY: as above, writable.
        unsafe {
            entry.write(RuntimeFunction {
                begin: offset(code.addr() + timed),
                end: offset(code.addr() + end),
                unwind: offset(self.unwind.addr()),
            });
        };
        self.count += 1;
    }
}

/// The timed thunks in front of every link and original: their code, slots
/// and counters.
struct Hops {
    code: *mut u8,
    slots: *mut AtomicUsize,
    counters: *mut Counter,
    count: usize,
}

impl Hops {
    /// Writes a hop for every link and original of `chains`, in that order.
    ///
    /// # Safety
    ///
    /// The areas hold `count` thunks, slots and counters, committed, zeroed,
    /// writable and never freed, within ±2 GiB of each other, and `table`
    /// has room for `count` more entries.
    unsafe fn arm(&self, chains: &mut Chains, table: &mut Table) {
        let mut next = 0;
        let mut hop = |detour: usize| -> Option<Hop> {
            let i = next;
            if i >= self.count {
                return None;
            }
            next += 1;
            // SAFETY: `i < count`, inside the slot area.
            let slot_ptr = unsafe { self.slots.add(i) };
            // SAFETY: committed, zeroed, aligned and never freed.
            let slot: &'static AtomicUsize = unsafe { &*slot_ptr };
            // SAFETY: `i < count`, inside the counter area.
            let counter_ptr = unsafe { self.counters.add(i) };
            // SAFETY: as for the slot; zero is a valid counter.
            let counter: &'static Counter = unsafe { &*counter_ptr };
            // SAFETY: `i < count`, inside the code area.
            let code = unsafe { self.code.add(i * TIMED_THUNK) };
            slot.store(detour, Ordering::Relaxed);
            // SAFETY: `code` has `TIMED_THUNK` writable bytes; the slot and
            // the counter are within ±2 GiB.
            let written =
                unsafe { write_thunk(code, TIMED_THUNK, slot, None, Some((counter, true))) };
            // SAFETY: the caller guarantees room in the table.
            unsafe { table.add(code, &written) };
            Some(Hop {
                entry: code.addr(),
                slot,
                counter,
            })
        };
        for chain in chains.iter_mut() {
            for link in &mut chain.links {
                link.hop = hop(link.detour);
            }
            chain.original_hop = hop(0);
        }
    }
}

/// Registers the unwind data of the timed calls, so a stack walk or an
/// exception passes through their frames.
///
/// # Safety
///
/// `table` holds `count` sorted entries naming code and unwind data inside
/// the block at `base`, all of it never freed.
unsafe fn register_unwind(base: *mut u8, table: *const RuntimeFunction, count: u32) {
    if count == 0 {
        return;
    }
    // SAFETY: forwarded from the caller.
    let added = unsafe {
        RtlAddFunctionTable(
            table.cast::<IMAGE_RUNTIME_FUNCTION_ENTRY>(),
            count,
            base.addr() as u64,
        )
    };
    if !added {
        crate::log!(WARN, "timed hook calls have no unwind data");
    }
}

/// Every target failed and every patch left out, for `reason`.
fn unarmed(chains: &mut Chains, mids: &Mids, patches: &[CodePatch], reason: &str) -> Armed {
    Armed {
        failures: chains
            .iter_mut()
            .map(|c| {
                // Never armed: a reload must not link into it.
                c.head = None;
                Failure::chain(c.target, reason)
            })
            .chain(
                mids.addresses()
                    .into_iter()
                    .map(|a| Failure::site(a, reason)),
            )
            .collect(),
        code: patches
            .iter()
            .map(|_| Verdict::Skip(reason.into()))
            .collect(),
    }
}

/// Detours each target to its thunk, wiring the chain's slots before the
/// target can jump there, jumps from each mid-function hook's address to its
/// stub, and writes the code patches that pass their check.
///
/// Every jump is written in one batch. Where two hooks would overwrite the
/// same bytes, the first keeps them: functions before mid-function hooks.
fn hook(
    chains: &mut Chains,
    mids: &Mids,
    thunks: &[(u64, *mut u8)],
    off: fn(usize) -> bool,
    patches: &[CodePatch],
    failures: &mut Vec<Failure>,
) -> Vec<Verdict> {
    let sites = mids.sites(off);
    let stubs = Stubs::dispatching(&sites);
    if stubs.is_none() {
        failures.extend(
            sites
                .iter()
                .map(|site| Failure::site(site.address, "no memory for stubs")),
        );
    }
    // Targets are addresses in this 64-bit process, so they fit a usize.
    let functions = thunks.iter().filter_map(|&(target, code)| {
        Some(Request {
            target: usize::try_from(target).ok()?,
            detour: code.addr(),
            inside: false,
        })
    });
    let inside = stubs.iter().flat_map(|stubs| {
        sites.iter().enumerate().filter_map(|(i, site)| {
            Some(Request {
                target: usize::try_from(site.address).ok()?,
                detour: stubs.entry(i),
                inside: true,
            })
        })
    });
    let requests: Vec<Request> = functions.chain(inside).collect();
    let pending = crate::detour::prepare(&requests);

    let mut ready: Vec<(u64, bool, &Pending)> = Vec::with_capacity(pending.len());
    let mut taken: Vec<(usize, usize)> = Vec::with_capacity(pending.len());
    for (request, result) in requests.iter().zip(&pending) {
        let target = request.target as u64;
        let result = result.as_ref().map_err(Clone::clone).and_then(|hook| {
            let (start, len) = hook.span();
            if taken.iter().any(|&(s, l)| s < start + len && start < s + l) {
                return Err("its bytes overlap another hook's jump".to_owned());
            }
            taken.push((start, len));
            Ok(hook)
        });
        match result {
            Ok(hook) if request.inside => {
                if let (Some(stubs), Some(i)) =
                    (&stubs, sites.iter().position(|s| s.address == target))
                {
                    stubs.finish(i, hook.original);
                    ready.push((target, true, hook));
                }
            }
            Ok(hook) => {
                if let Some(chain) = chains.iter_mut().find(|c| c.target == target) {
                    chain.original = hook.original;
                    // Wired before the target can jump to its thunk.
                    chain.wire(off);
                    ready.push((target, false, hook));
                }
            }
            Err(reason) => {
                if !request.inside
                    && let Some(chain) = chains.iter_mut().find(|c| c.target == target)
                {
                    chain.head = None;
                }
                failures.push(Failure {
                    target,
                    inside: request.inside,
                    reason,
                });
            }
        }
    }
    if let Some(stubs) = &stubs
        && !stubs.seal()
    {
        ready.retain(|&(target, inside, _)| {
            if inside {
                failures.push(Failure::site(target, "stub pages not executable"));
            }
            !inside
        });
    }
    write(chains, mids, &ready, off, patches, failures)
}

/// The mods hooked on a target that failed, function or mid-function hook;
/// never the loader.
fn failed_owners(failures: &[Failure], chains: &Chains, mids: &Mids) -> Vec<usize> {
    failures
        .iter()
        .flat_map(|f| {
            if f.inside {
                mids.owners(f.target)
            } else {
                chains.owners(f.target)
            }
        })
        .filter(|&owner| owner != crate::chains::LOADER)
        .collect()
}

/// Writes the prepared jumps and the code patches that pass their check, all
/// or none.
fn write(
    chains: &mut Chains,
    mids: &Mids,
    ready: &[(u64, bool, &Pending)],
    off: fn(usize) -> bool,
    patches: &[CodePatch],
    failures: &mut Vec<Failure>,
) -> Vec<Verdict> {
    let hooks: Vec<&Pending> = ready.iter().map(|&(_, _, hook)| hook).collect();
    let hooked: Vec<(usize, usize)> = hooks.iter().map(|h| h.span()).collect();
    // A mod whose hook failed is turned off: its patches stay out with it.
    let failed = failed_owners(failures, chains, mids);
    let off_or_failed = |owner: usize| off(owner) || failed.contains(&owner);
    let mut verdicts = crate::code::check(patches, &hooked, off_or_failed, crate::code::read);
    let writes: Vec<crate::detour::Bytes<'_>> = patches
        .iter()
        .zip(&verdicts)
        .filter(|&(_, v)| *v == Verdict::Write)
        .map(|(p, _)| crate::detour::Bytes {
            address: p.address,
            bytes: &p.replacement,
        })
        .collect();
    let targets: Vec<u64> = ready
        .iter()
        .filter(|&&(_, inside, _)| !inside)
        .map(|&(target, _, _)| target)
        .collect();
    crate::code::arming(patches, &verdicts, &targets);
    if !crate::detour::commit(&hooks, &writes) {
        crate::code::arming(&[], &[], &[]);
        for verdict in &mut verdicts {
            if *verdict == Verdict::Write {
                *verdict = Verdict::Skip("the code could not be written".into());
            }
        }
        for &(target, inside, _) in ready {
            if !inside && let Some(chain) = chains.iter_mut().find(|c| c.target == target) {
                chain.head = None;
            }
            failures.push(Failure {
                target,
                inside,
                reason: "the target could not be written".into(),
            });
        }
    }
    verdicts
}

/// An entry of a function table for `RtlAddFunctionTable`: offsets from
/// the table's base.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct RuntimeFunction {
    begin: u32,
    end: u32,
    unwind: u32,
}

/// What [`write_thunk`] wrote.
#[derive(Clone, Copy, Debug, Default)]
struct Written {
    /// The timed call's start and end in the thunk, when it has one.
    timed: Option<(usize, usize)>,
}

/// Machine code with rip-relative operands, written for one address.
struct Asm {
    at: usize,
    bytes: Vec<u8>,
}

impl Asm {
    fn emit(&mut self, bytes: &[u8]) {
        self.bytes.extend_from_slice(bytes);
    }

    /// `prefix`, the displacement from the instruction's end to `target`,
    /// then `tail`.
    fn rip(&mut self, prefix: &[u8], target: usize, tail: &[u8]) {
        let end = self.at + self.bytes.len() + prefix.len() + 4 + tail.len();
        self.emit(prefix);
        self.emit(&slot_offset(target, end).to_le_bytes());
        self.emit(tail);
    }

    /// `op [rsp + displacement]` with a 32-bit displacement.
    fn stack(&mut self, op: &[u8], displacement: u32) {
        self.emit(op);
        self.emit(&[0x24]);
        self.emit(&displacement.to_le_bytes());
    }

    /// `rdtsc` into `rax` as one 64-bit value; `rdx` is lost.
    fn ticks(&mut self) {
        self.emit(&[0x0f, 0x31]); // rdtsc
        self.emit(&[0x48, 0xc1, 0xe2, 0x20]); // shl rdx, 32
        self.emit(&[0x48, 0x09, 0xd0]); // or rax, rdx
    }

    fn len(&self) -> usize {
        self.bytes.len()
    }
}

/// Writes the thunk: `mov gs:[TlsSlots + index * 8], reg` for each saved
/// register of a frame chain, `lock inc [calls]` when counting, then `jmp
/// [rip + slot]`; when timing, every 256th call takes the timed call instead.
///
/// # Safety
///
/// `code` has `len` writable bytes, and `slot` and the counter lie within
/// ±2 GiB of it.
unsafe fn write_thunk(
    code: *mut u8,
    len: usize,
    slot: &AtomicUsize,
    frame: Option<[u32; 9]>,
    counter: Option<(&Counter, bool)>,
) -> Written {
    let slot = core::ptr::from_ref(slot).addr();
    let mut asm = Asm {
        at: code.addr(),
        bytes: Vec::with_capacity(len),
    };
    if let Some(indices) = frame {
        for (&(rex, modrm), index) in SAVED.iter().zip(indices) {
            let displacement = TEB_TLS_SLOTS + index * 8;
            asm.emit(&[0x65, rex, 0x89, modrm, 0x25]);
            asm.emit(&displacement.to_le_bytes());
        }
    }
    let mut written = Written::default();
    match counter {
        None => asm.rip(&[0xff, 0x25], slot, &[]), // jmp [rip + slot]
        Some((counter, timed)) => {
            let calls = core::ptr::from_ref(&counter.calls).addr();
            asm.rip(&[0xf0, 0x48, 0xff, 0x05], calls, &[]); // lock inc qword [calls]
            if timed {
                asm.rip(&[0xf6, 0x05], calls, &[0xff]); // test byte [calls], 0xff
                asm.emit(&[0x74, 0x06]); // jz to the timed call
            }
            asm.rip(&[0xff, 0x25], slot, &[]); // jmp [rip + slot]
            if timed {
                let start = asm.len();
                timed_call(&mut asm, slot, counter);
                written.timed = Some((start, asm.len()));
            }
        }
    }
    let mut bytes = asm.bytes;
    bytes.resize(len, 0xcc);
    // SAFETY: the caller guarantees `len` writable bytes at `code`.
    unsafe { code.copy_from_nonoverlapping(bytes.as_ptr(), len) };
    written
}

/// The timed call: a frame with the caller's stack arguments, the chain
/// called through its slot between two time stamps, the ticks added to the
/// counter. Keeps every argument register and the result in `rax` and
/// `xmm0`; uses `rax`, `rdx` and `r10`, which carry no argument here.
fn timed_call(asm: &mut Asm, slot: usize, counter: &Counter) {
    asm.emit(&[0x48, 0x81, 0xec]); // sub rsp, FRAME
    asm.emit(&FRAME.to_le_bytes());
    for i in 0..STACK_ARGS {
        asm.stack(&[0x48, 0x8b, 0x84], FRAME + 0x28 + i * 8); // mov rax, [caller's argument]
        asm.stack(&[0x48, 0x89, 0x84], 0x20 + i * 8); // mov [our argument], rax
    }
    asm.emit(&[0x4c, 0x8b, 0xd2]); // mov r10, rdx
    asm.ticks();
    asm.stack(&[0x48, 0x89, 0x84], START); // mov [rsp + START], rax
    asm.emit(&[0x4c, 0x89, 0xd2]); // mov rdx, r10
    asm.rip(&[0xff, 0x15], slot, &[]); // call [rip + slot]
    asm.stack(&[0x48, 0x89, 0x84], SAVED_RESULT); // mov [rsp + SAVED_RESULT], rax
    asm.ticks();
    asm.stack(&[0x48, 0x2b, 0x84], START); // sub rax, [rsp + START]
    let ticks = core::ptr::from_ref(&counter.ticks).addr();
    let samples = core::ptr::from_ref(&counter.samples).addr();
    asm.rip(&[0xf0, 0x48, 0x01, 0x05], ticks, &[]); // lock add [ticks], rax
    asm.rip(&[0xf0, 0x48, 0xff, 0x05], samples, &[]); // lock inc qword [samples]
    asm.stack(&[0x48, 0x8b, 0x84], SAVED_RESULT); // mov rax, [rsp + SAVED_RESULT]
    asm.emit(&[0x48, 0x81, 0xc4]); // add rsp, FRAME
    asm.emit(&FRAME.to_le_bytes());
    asm.emit(&[0xc3]); // ret
}

fn slot_offset(slot: usize, next_instruction: usize) -> i32 {
    let delta = slot.wrapping_sub(next_instruction).cast_signed();
    i32::try_from(delta).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicU64;

    use super::*;

    #[test]
    fn a_thunk_jumps_through_its_slot() {
        let slot = AtomicUsize::new(0);
        let mut code = [0u8; THUNK];
        let base = code.as_mut_ptr();
        // SAFETY: a local buffer of `THUNK` bytes; the slot is on the same stack.
        unsafe { write_thunk(base, THUNK, &slot, None, None) };
        assert_eq!(code[..2], [0xff, 0x25]);
        let relative = i32::from_le_bytes(code[2..6].try_into().unwrap());
        let target = (base.addr() + 6).wrapping_add_signed(relative as isize);
        assert_eq!(target, core::ptr::from_ref(&slot).addr());
    }

    #[test]
    fn a_frame_thunk_saves_the_stack_pointer_first() {
        let slot = AtomicUsize::new(0);
        let mut code = [0u8; THUNK];
        // SAFETY: as above.
        unsafe {
            write_thunk(
                code.as_mut_ptr(),
                THUNK,
                &slot,
                Some([3, 4, 5, 6, 7, 8, 9, 10, 11]),
                None,
            );
        }
        assert_eq!(code[..5], [0x65, 0x48, 0x89, 0x24, 0x25]);
        assert_eq!(
            u32::from_le_bytes(code[5..9].try_into().unwrap()),
            0x1480 + 24
        );
        // r14, the eighth register, into slot 10.
        assert_eq!(code[63..68], [0x65, 0x4c, 0x89, 0x34, 0x25]);
        assert_eq!(
            u32::from_le_bytes(code[68..72].try_into().unwrap()),
            0x1480 + 80
        );
        assert_eq!(code[81..83], [0xff, 0x25]);
    }

    #[test]
    fn a_counting_frame_thunk_still_fits() {
        let slot = AtomicUsize::new(0);
        let counter = Counter::default();
        let mut code = [0u8; THUNK];
        // SAFETY: as above.
        let written = unsafe {
            write_thunk(
                code.as_mut_ptr(),
                THUNK,
                &slot,
                Some([3, 4, 5, 6, 7, 8, 9, 10, 11]),
                Some((&counter, false)),
            )
        };
        assert!(written.timed.is_none());
        assert_eq!(code[81..85], [0xf0, 0x48, 0xff, 0x05]);
        assert_eq!(code[89..91], [0xff, 0x25]);
    }

    /// Executable memory: a thunk at the start, its slot and counter after it.
    struct Page {
        base: *mut u64,
    }

    impl Page {
        fn new() -> Self {
            // SAFETY: a fresh allocation, written before it runs.
            let base = unsafe {
                VirtualAlloc(
                    core::ptr::null(),
                    PAGE,
                    MEM_RESERVE | MEM_COMMIT,
                    windows_sys::Win32::System::Memory::PAGE_EXECUTE_READWRITE,
                )
            }
            .cast::<u64>();
            assert!(!base.is_null());
            Self { base }
        }

        fn slot(&self) -> &'static AtomicUsize {
            // SAFETY: inside the page.
            let slot = unsafe { self.base.add(TIMED_THUNK / 8) }.cast::<AtomicUsize>();
            // SAFETY: an aligned word of the page, which is never freed.
            unsafe { &*slot }
        }

        fn counter(&self) -> &'static Counter {
            // SAFETY: inside the page.
            let counter = unsafe { self.base.add(TIMED_THUNK / 8 + 1) }.cast::<Counter>();
            // SAFETY: aligned words of the page, which is never freed.
            unsafe { &*counter }
        }

        /// The thunk, timed or only counting, jumping to `target`.
        fn thunk(&self, target: usize, timed: bool) -> usize {
            self.slot().store(target, Ordering::Release);
            // SAFETY: the page has `TIMED_THUNK` bytes before the slot.
            let written = unsafe {
                write_thunk(
                    self.base.cast(),
                    TIMED_THUNK,
                    self.slot(),
                    None,
                    Some((self.counter(), timed)),
                )
            };
            assert_eq!(written.timed.is_some(), timed);
            self.base.addr()
        }
    }

    type Eight = extern "system" fn(u64, u64, u64, u64, u64, u64, u64, u64) -> u64;

    extern "system" fn weigh(
        w1: u64,
        w2: u64,
        w3: u64,
        w4: u64,
        w5: u64,
        w6: u64,
        w7: u64,
        w8: u64,
    ) -> u64 {
        w1 + 2 * w2 + 3 * w3 + 4 * w4 + 5 * w5 + 6 * w6 + 7 * w7 + 8 * w8
    }

    extern "system" fn halve(value: f64) -> f64 {
        value / 2.0
    }

    #[test]
    fn a_timed_thunk_counts_every_call_and_times_one_in_256() {
        let page = Page::new();
        let thunk = page.thunk((weigh as Eight as *const ()).addr(), true);
        // SAFETY: the thunk forwards to `weigh`, with its signature.
        let call = unsafe { core::mem::transmute::<usize, Eight>(thunk) };
        for i in 0..600 {
            assert_eq!(call(1, 1, 1, 1, 1, 1, 1, i), 28 + 8 * i);
        }
        let counter = page.counter();
        let load = |value: &AtomicU64| value.load(Ordering::Relaxed);
        assert_eq!(load(&counter.calls), 600);
        // The 256th and the 512th.
        assert_eq!(load(&counter.samples), 2);
        assert!(load(&counter.ticks) > 0);
    }

    #[test]
    fn a_timed_call_keeps_a_floating_point_result() {
        let page = Page::new();
        let thunk = page.thunk(
            (halve as extern "system" fn(f64) -> f64 as *const ()).addr(),
            true,
        );
        // SAFETY: the thunk forwards to `halve`, with its signature.
        let call = unsafe { core::mem::transmute::<usize, extern "system" fn(f64) -> f64>(thunk) };
        for _ in 0..512 {
            assert!((call(3.0) - 1.5).abs() < f64::EPSILON);
        }
        let samples = page.counter().samples.load(Ordering::Relaxed);
        assert_eq!(samples, 2);
    }

    #[test]
    fn a_counting_thunk_counts_without_timing() {
        let page = Page::new();
        let thunk = page.thunk((weigh as Eight as *const ()).addr(), false);
        // SAFETY: the thunk forwards to `weigh`, with its signature.
        let call = unsafe { core::mem::transmute::<usize, Eight>(thunk) };
        for _ in 0..300 {
            assert_eq!(call(1, 0, 0, 0, 0, 0, 0, 1), 9);
        }
        let counter = page.counter();
        assert_eq!(counter.calls.load(Ordering::Relaxed), 300);
        assert_eq!(counter.samples.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn the_owners_of_a_failed_mid_function_hook_count_as_failed() {
        use crate::chains::{LOADER, Link};
        use crate::mid::MidLink;
        use enshwick_loader_api::abi::{HookFlags, Registers};

        static NEXT: AtomicUsize = AtomicUsize::new(0);
        unsafe extern "system" fn handler(_: *mut Registers, _: *mut core::ffi::c_void) {}

        let mut chains = Chains::default();
        let link = |owner| Link {
            owner,
            detour: 0x1,
            next: &NEXT,
            priority: 0,
            flags: HookFlags::NONE,
            hop: None,
        };
        let _ = chains.add(0x10, link(LOADER));
        let _ = chains.add(0x10, link(2));
        let mut mids = Mids::default();
        let mid = MidLink {
            owner: 1,
            handler,
            user: 0,
            priority: 0,
            flags: HookFlags::NONE,
        };
        let _ = mids.add(0x20, mid);
        let failures = [Failure::chain(0x10, "x"), Failure::site(0x20, "y")];
        assert_eq!(failed_owners(&failures, &chains, &mids), [2, 1]);
    }

    #[test]
    fn the_unwind_data_describes_the_frame() {
        assert_eq!(FRAME % 16, 8);
        assert_eq!(u32::from(UNWIND[6]) * 8, FRAME);
        assert_eq!(UNWIND[1], PROLOG);
    }
}
