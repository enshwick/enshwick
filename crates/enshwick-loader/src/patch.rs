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

use std::sync::atomic::AtomicUsize;

use windows_sys::Win32::System::Diagnostics::Debug::FlushInstructionCache;
use windows_sys::Win32::System::Memory::{
    MEM_COMMIT, MEM_RESERVE, PAGE_EXECUTE_READ, PAGE_READWRITE, VirtualAlloc, VirtualProtect,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, TlsAlloc, TlsGetValue};

use crate::chains::Chains;
use crate::code::{CodePatch, Verdict};
use crate::detour::{Pending, Request};
use crate::mid::{Mids, Stubs};

const PAGE: usize = 4096;
const THUNK: usize = 96;
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
    pub(crate) fn read(self) -> enshwick_api::abi::CallerFrame {
        // SAFETY: each index was allocated by `allocate`.
        let value = |i: usize| unsafe { TlsGetValue(self.0[i]) }.addr() as u64;
        enshwick_api::abi::CallerFrame {
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
) -> Armed {
    let count = chains.len();
    if count == 0 {
        let mut armed = Armed::default();
        armed.code = hook(chains, mids, &[], off, patches, &mut armed.failures);
        return armed;
    }
    let code_len = (count * THUNK).div_ceil(PAGE) * PAGE;
    let data_len = (count * size_of::<usize>()).div_ceil(PAGE) * PAGE;
    // SAFETY: a fresh reservation; null on failure.
    let block = unsafe {
        VirtualAlloc(
            core::ptr::null(),
            code_len + data_len,
            MEM_RESERVE | MEM_COMMIT,
            PAGE_READWRITE,
        )
    }
    .cast::<u8>();
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

    let mut failures = Vec::new();
    let mut thunks = Vec::with_capacity(count);
    for (i, chain) in chains.iter_mut().enumerate() {
        // SAFETY: `i < count`, inside the data pages.
        let slot_ptr = unsafe { data.add(i) };
        // SAFETY: committed, zeroed and aligned memory that is never freed.
        let slot: &'static AtomicUsize = unsafe { &*slot_ptr };
        // SAFETY: `i < count`, inside the code pages.
        let code = unsafe { block.add(i * THUNK) };
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
        // SAFETY: `code` has `THUNK` writable bytes; `slot` is within ±2 GiB.
        unsafe { write_thunk(code, slot, frame_index) };
        chain.head = Some(slot);
        thunks.push((chain.target, code));
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

/// Every target failed and every patch left out, for `reason`.
fn unarmed(chains: &mut Chains, mids: &Mids, patches: &[CodePatch], reason: &str) -> Armed {
    Armed {
        failures: chains
            .iter_mut()
            .map(|c| Failure::chain(c.target, reason))
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
    write(chains, &ready, off, patches, failures)
}

/// Writes the prepared jumps and the code patches that pass their check, all
/// or none.
fn write(
    chains: &mut Chains,
    ready: &[(u64, bool, &Pending)],
    off: fn(usize) -> bool,
    patches: &[CodePatch],
    failures: &mut Vec<Failure>,
) -> Vec<Verdict> {
    let hooks: Vec<&Pending> = ready.iter().map(|&(_, _, hook)| hook).collect();
    let hooked: Vec<(usize, usize)> = hooks.iter().map(|h| h.span()).collect();
    // A mod whose hook failed is turned off: its patches stay out with it.
    let failed: Vec<usize> = failures
        .iter()
        .flat_map(|f| chains.owners(f.target))
        .filter(|&owner| owner != crate::chains::LOADER)
        .collect();
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

/// Writes `mov gs:[TlsSlots + index * 8], reg` for each saved register, then
/// `jmp [rip + slot]`.
///
/// # Safety
///
/// `code` has `THUNK` writable bytes and `slot` lies within ±2 GiB of it.
unsafe fn write_thunk(code: *mut u8, slot: &AtomicUsize, frame: Option<[u32; 9]>) {
    let mut bytes = [0xcc_u8; THUNK];
    let mut at = 0;
    if let Some(indices) = frame {
        for (&(rex, modrm), index) in SAVED.iter().zip(indices) {
            let displacement = TEB_TLS_SLOTS + index * 8;
            bytes[at..at + 5].copy_from_slice(&[0x65, rex, 0x89, modrm, 0x25]);
            bytes[at + 5..at + 9].copy_from_slice(&displacement.to_le_bytes());
            at += 9;
        }
    }
    bytes[at..at + 2].copy_from_slice(&[0xff, 0x25]);
    let next_instruction = code.addr() + at + 6;
    let relative = slot_offset(core::ptr::from_ref(slot).addr(), next_instruction);
    bytes[at + 2..at + 6].copy_from_slice(&relative.to_le_bytes());
    // SAFETY: the caller guarantees `THUNK` writable bytes at `code`.
    unsafe { code.copy_from_nonoverlapping(bytes.as_ptr(), THUNK) };
}

fn slot_offset(slot: usize, next_instruction: usize) -> i32 {
    let delta = slot.wrapping_sub(next_instruction).cast_signed();
    i32::try_from(delta).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_thunk_jumps_through_its_slot() {
        let slot = AtomicUsize::new(0);
        let mut code = [0u8; THUNK];
        let base = code.as_mut_ptr();
        // SAFETY: a local buffer of `THUNK` bytes; the slot is on the same stack.
        unsafe { write_thunk(base, &slot, None) };
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
                &slot,
                Some([3, 4, 5, 6, 7, 8, 9, 10, 11]),
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
}
