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
    IMAGE_RUNTIME_FUNCTION_ENTRY, RtlAddFunctionTable,
};
use windows_sys::Win32::System::Memory::{MEM_COMMIT, MEM_RESERVE, PAGE_READWRITE, VirtualAlloc};
use windows_sys::Win32::System::Threading::{TlsAlloc, TlsGetValue};

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
            // SAFETY: `TlsAlloc` takes no arguments; a failure returns
            // `TLS_OUT_OF_INDEXES`, checked below.
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
    pub(crate) fn read(self) -> enshwick::abi::CallerFrame {
        // SAFETY: `allocate` built `self` only from indices `TlsAlloc` gave
        // and the loader never frees; `i` is under their nine.
        let value = |i: usize| unsafe { TlsGetValue(self.0[i]) }.addr() as u64;
        enshwick::abi::CallerFrame {
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
    // SAFETY: `rdtsc` reads the time stamp counter, present on every
    // x86-64 processor, and touches no memory.
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
    // SAFETY: `block` is non-null, checked by the guard before it, and
    // `reserve` committed `code_len + data_len` bytes there, so `code_len`
    // stays inside.
    let data_start = unsafe { block.add(code_len) };
    #[expect(
        clippy::cast_ptr_alignment,
        reason = "the data pages start on a page boundary"
    )]
    let data = data_start.cast::<AtomicUsize>();
    // SAFETY: the data pages hold `data_len` bytes, at least `slots_len +
    // counters_len + functions_len + UNWIND.len()`, so `slots_len` stays
    // inside.
    let counter_start = unsafe { data_start.add(slots_len) };
    #[expect(
        clippy::cast_ptr_alignment,
        reason = "the slots end on an 8-byte boundary"
    )]
    let counter_base = counter_start.cast::<Counter>();
    // SAFETY: the data pages hold at least `slots_len + counters_len +
    // functions_len + UNWIND.len()` bytes, so this offset stays inside.
    let function_start = unsafe { data_start.add(slots_len + counters_len) };
    #[expect(
        clippy::cast_ptr_alignment,
        reason = "the counters end on an 8-byte boundary"
    )]
    let function_base = function_start.cast::<RuntimeFunction>();
    // SAFETY: the data pages hold at least `slots_len + counters_len +
    // functions_len + UNWIND.len()` bytes, so this offset stays inside.
    let unwind = unsafe { data_start.add(slots_len + counters_len + functions_len) };
    // SAFETY: `data_len` leaves `UNWIND.len()` read-write bytes at `unwind`,
    // which nothing else uses yet; `UNWIND` is a constant, so the ranges do
    // not overlap.
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
        // SAFETY: `i < count` and the slot area holds `count + hops` words,
        // so slot `i` lies inside the data pages.
        let slot_ptr = unsafe { data.add(i) };
        // SAFETY: `reserve` committed and zeroed the block, and the loader never
        // frees it; the data pages start on a page
        // boundary, so the word is aligned, and a zeroed `AtomicUsize` is
        // valid.
        let slot: &'static AtomicUsize = unsafe { &*slot_ptr };
        // SAFETY: `i < count` and the code pages hold `count * thunk` bytes at
        // least, so thunk `i` lies inside them.
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
            // SAFETY: `i < count` and the counter area holds `count + hops`
            // counters, so counter `i` lies inside the data pages.
            let counter_ptr = unsafe { counter_base.add(i) };
            // SAFETY: `reserve` committed and zeroed the block, and the
            // loader never frees it; the slots end on an 8-byte boundary,
            // and a zeroed `Counter` is valid.
            let counter: &'static Counter = unsafe { &*counter_ptr };
            counters.push((chain.target, counter));
            (counter, counting == Count::Timed)
        });
        // SAFETY: thunk `i` has `thunk` read-write bytes at `code` in the
        // block, which nothing runs yet, and the slot and the counter lie in
        // the same block, a few pages per chain, far under 2 GiB.
        let written = unsafe { write_thunk(code, thunk, slot, frame_index, counter) };
        // SAFETY: the function table has room for `count + hops` entries and
        // gets at most one per thunk; `code` and the unwind data lie in
        // `block`.
        unsafe { table.add(code, &written) };
        chain.head = Some(slot);
        thunks.push((chain.target, code));
    }
    if hops > 0 {
        let hop_code = Hops {
            // SAFETY: the code pages hold `count * thunk + hops *
            // TIMED_THUNK` bytes at least, so the hops' thunks start inside.
            code: unsafe { block.add(count * thunk) },
            // SAFETY: the slot area holds `count + hops` words, so the hops'
            // slots start inside the data pages.
            slots: unsafe { data.add(count) },
            // SAFETY: the counter area holds `count + hops` counters, so the
            // hops' counters start inside the data pages.
            counters: unsafe { counter_base.add(count) },
            count: hops,
        };
        // SAFETY: the hop areas hold `hops` thunks, slots and counters each,
        // committed, zeroed, read-write and never freed, in one block far
        // under 2 GiB; the table has room for `count + hops` entries and the heads
        // took at most `count`.
        unsafe { hop_code.arm(chains, &mut table) };
    }
    // SAFETY: `table.add` wrote `table.count` entries in thunk order, so
    // sorted by address, naming code and unwind data inside `block`, which
    // the loader never frees.
    unsafe { register_unwind(block, function_base, table.count) };
    if counting != Count::Off {
        let _first = COUNTERS.set(counters.into_boxed_slice());
        let _first = ARMED_AT.set((rdtsc(), std::time::Instant::now()));
    }

    // SAFETY: `block` and `code_len` are the code pages `reserve` gave this
    // batch, written in full; no thread runs them before `hook` links them.
    if !unsafe { crate::pages::seal(block.expose_provenance(), code_len) } {
        return unarmed(chains, mids, patches, "thunk pages not executable");
    }

    let code = hook(chains, mids, &thunks, off, patches, &mut failures);
    Armed { failures, code }
}

/// `len` bytes of fresh read-write memory; null when there is none.
fn reserve(len: usize) -> *mut u8 {
    // SAFETY: `VirtualAlloc` with a null address reserves fresh pages and
    // touches no memory of ours; null on failure, which callers check.
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
    /// - `entries` has room for one more writable, aligned entry.
    /// - `code` and the unwind data lie inside the block at `base`.
    unsafe fn add(&mut self, code: *mut u8, written: &Written) {
        let Some((timed, end)) = written.timed else {
            return;
        };
        let offset = |at: usize| u32::try_from(at - self.base.addr()).unwrap_or(0);
        // SAFETY: `entries` has room for entry `count` by this function's
        // `# Safety`, so the offset stays inside the table.
        let entry = unsafe { self.entries.add(self.count as usize) };
        // SAFETY: entry `count` is writable and aligned by this function's
        // `# Safety`, and nothing reads it yet.
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
    /// - `code`, `slots` and `counters` each hold `count` entries, committed,
    ///   zeroed, writable, aligned and never freed.
    /// - The three areas lie within 2 GiB of each other.
    /// - `table` has room for `count` more entries.
    unsafe fn arm(&self, chains: &mut Chains, table: &mut Table) {
        let mut next = 0;
        let mut hop = |detour: usize| -> Option<Hop> {
            let i = next;
            if i >= self.count {
                return None;
            }
            next += 1;
            // SAFETY: `i < self.count`, checked by the guard before it, and the
            // slot area holds `count` slots by this function's `# Safety`.
            let slot_ptr = unsafe { self.slots.add(i) };
            // SAFETY: slot `i` is committed, zeroed, aligned and never freed
            // by this function's `# Safety`, and a zeroed `AtomicUsize` is
            // valid.
            let slot: &'static AtomicUsize = unsafe { &*slot_ptr };
            // SAFETY: `i < self.count`, checked by the guard before it, and the
            // counter area holds `count` counters by this function's `#
            // Safety`.
            let counter_ptr = unsafe { self.counters.add(i) };
            // SAFETY: counter `i` is committed, zeroed, aligned and never
            // freed by this function's `# Safety`, and a zeroed `Counter` is
            // valid.
            let counter: &'static Counter = unsafe { &*counter_ptr };
            // SAFETY: `i < self.count`, checked by the guard before it, and the
            // code area holds `count` thunks of `TIMED_THUNK` bytes by this
            // function's `# Safety`.
            let code = unsafe { self.code.add(i * TIMED_THUNK) };
            slot.store(detour, Ordering::Relaxed);
            // SAFETY: thunk `i` has `TIMED_THUNK` writable bytes at `code`,
            // and the slot and the counter lie within 2 GiB of it, by this
            // function's `# Safety`.
            let written =
                unsafe { write_thunk(code, TIMED_THUNK, slot, None, Some((counter, true))) };
            // SAFETY: `table` has room for one entry per hop by this
            // function's `# Safety`, and `code` lies in its block.
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
/// - `table` holds `count` entries sorted by address, aligned for
///   `IMAGE_RUNTIME_FUNCTION_ENTRY`.
/// - Each entry names code and unwind data inside the block at `base`.
/// - The table, the code and the unwind data are never freed.
unsafe fn register_unwind(base: *mut u8, table: *const RuntimeFunction, count: u32) {
    if count == 0 {
        return;
    }
    // SAFETY: `table` holds `count` sorted entries naming code and unwind
    // data in the never-freed block at `base`, by this function's
    // `# Safety`, which is what `RtlAddFunctionTable` keeps a pointer to.
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
/// - `code` has `len` writable bytes that no thread runs or reads during
///   the call.
/// - `slot` and the counter lie within 2 GiB of `code`.
unsafe fn write_thunk(
    code: *mut u8,
    len: usize,
    slot: &AtomicUsize,
    frame: Option<[u32; 9]>,
    counter: Option<(&Counter, bool)>,
) -> Written {
    let slot = core::ptr::from_ref(slot).expose_provenance();
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
            let calls = core::ptr::from_ref(&counter.calls).expose_provenance();
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
    // SAFETY: `code` has `len` writable bytes by this function's `# Safety`,
    // and `bytes` is a fresh vector of `len` bytes, so the ranges do not
    // overlap.
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
    let ticks = core::ptr::from_ref(&counter.ticks).expose_provenance();
    let samples = core::ptr::from_ref(&counter.samples).expose_provenance();
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
        // SAFETY: `code` is a local buffer of `THUNK` writable bytes, and
        // `slot` sits on the same stack, well within 2 GiB.
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
        // SAFETY: `code` is a local buffer of `THUNK` writable bytes, and
        // `slot` sits on the same stack, well within 2 GiB.
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
        // SAFETY: `code` is a local buffer of `THUNK` writable bytes, and
        // `slot` and `counter` sit on the same stack, well within 2 GiB.
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
            // SAFETY: `VirtualAlloc` with a null address reserves a fresh
            // page and touches no memory of ours; null on failure, asserted
            // below.
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
            // SAFETY: `TIMED_THUNK` bytes are under `PAGE`, so word
            // `TIMED_THUNK / 8` lies inside the page at `self.base`.
            let slot = unsafe { self.base.add(TIMED_THUNK / 8) }.cast::<AtomicUsize>();
            // SAFETY: the word is aligned in the committed, zeroed page,
            // which the test never frees, and a zeroed `AtomicUsize` is
            // valid.
            unsafe { &*slot }
        }

        fn counter(&self) -> &'static Counter {
            // SAFETY: `TIMED_THUNK` bytes and a slot are under `PAGE`, so
            // word `TIMED_THUNK / 8 + 1` lies inside the page at `self.base`.
            let counter = unsafe { self.base.add(TIMED_THUNK / 8 + 1) }.cast::<Counter>();
            // SAFETY: the words are aligned in the committed, zeroed page,
            // which the test never frees, a `Counter` fits after the slot,
            // and a zeroed `Counter` is valid.
            unsafe { &*counter }
        }

        /// The thunk, timed or only counting, jumping to `target`.
        fn thunk(&self, target: usize, timed: bool) -> usize {
            self.slot().store(target, Ordering::Release);
            // SAFETY: the page has `TIMED_THUNK` writable bytes before the
            // slot, which no thread runs yet, and the slot and the counter
            // lie in the same page.
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
        // SAFETY: `page.thunk` wrote a thunk that jumps through its slot to
        // `weigh`, so it has `Eight`'s signature.
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
        // SAFETY: `page.thunk` wrote a thunk that jumps through its slot to
        // `halve`, so it has `halve`'s signature.
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
        // SAFETY: `page.thunk` wrote a thunk that jumps through its slot to
        // `weigh`, so it has `Eight`'s signature.
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
        use enshwick::abi::{HookFlags, Registers};

        static NEXT: [AtomicUsize; 2] = [const { AtomicUsize::new(0) }; 2];
        extern "system" fn handler(_: *mut Registers, _: *mut core::ffi::c_void) {}

        let mut chains = Chains::default();
        let link = |owner: usize| Link {
            owner,
            detour: 0x1,
            next: &NEXT[usize::from(owner == LOADER)],
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
