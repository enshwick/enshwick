//! Arming: one detour per target, through a loader-owned thunk.
//!
//! Each target is detoured once, to a thunk that jumps through the chain's head
//! slot. Thunks sit on pages that are made execute-only once written; the slots
//! live on separate read-write pages, so rewiring never writes code. A thunk
//! for a chain that reads the caller frame first stores the stack pointer in a
//! TLS slot of the thread.

use std::sync::atomic::AtomicUsize;

use minhook::MinHook;
use windows_sys::Win32::System::Diagnostics::Debug::FlushInstructionCache;
use windows_sys::Win32::System::Memory::{
    MEM_COMMIT, MEM_RESERVE, PAGE_EXECUTE_READ, PAGE_READWRITE, VirtualAlloc, VirtualProtect,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, TlsAlloc, TlsGetValue};

use crate::chains::Chains;

const PAGE: usize = 4096;
const THUNK: usize = 32;
/// `TEB.TlsSlots` on x64; the first 64 TLS indices live there.
const TEB_TLS_SLOTS: u32 = 0x1480;
const TLS_INLINE_SLOTS: u32 = 64;
const TLS_OUT_OF_INDEXES: u32 = u32::MAX;

/// The TLS index that holds the caller frame, if one could be had.
#[derive(Clone, Copy, Debug)]
pub(crate) struct FrameSlot(u32);

impl FrameSlot {
    /// Allocates the index; `None` when no index the thunk can address is free.
    #[must_use]
    pub(crate) fn allocate() -> Option<Self> {
        // SAFETY: no preconditions.
        let index = unsafe { TlsAlloc() };
        (index != TLS_OUT_OF_INDEXES && index < TLS_INLINE_SLOTS).then_some(Self(index))
    }

    /// The caller frame the thunk stored on this thread.
    #[must_use]
    pub(crate) fn read(self) -> usize {
        // SAFETY: the index was allocated by `allocate`.
        unsafe { TlsGetValue(self.0) }.addr()
    }
}

/// A target that could not be armed, and why.
#[derive(Debug)]
pub(crate) struct Failure {
    /// The game function.
    pub(crate) target: u64,
    /// For the log.
    pub(crate) reason: String,
}

/// Detours every chain's target to its thunk and wires the slots.
///
/// Returns the targets that failed; their chains stay unarmed and the game
/// calls those functions unchanged.
pub(crate) fn arm(
    chains: &mut Chains,
    frame: Option<FrameSlot>,
    off: impl Fn(usize) -> bool + Copy,
) -> Vec<Failure> {
    let count = chains.len();
    if count == 0 {
        return Vec::new();
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
        return chains
            .iter_mut()
            .map(|c| Failure {
                target: c.target,
                reason: "no memory for thunks".into(),
            })
            .collect();
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
                failures.push(Failure {
                    target: chain.target,
                    reason: "no TLS slot for the caller frame".into(),
                });
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
        return chains
            .iter_mut()
            .map(|c| Failure {
                target: c.target,
                reason: "thunk pages not executable".into(),
            })
            .collect();
    }
    // SAFETY: no preconditions.
    let process = unsafe { GetCurrentProcess() };
    // SAFETY: flushing our own process's freshly written code.
    unsafe { FlushInstructionCache(process, block.cast(), code_len) };

    for chain in chains.iter_mut() {
        let Some(&(_, code)) = thunks.iter().find(|(t, _)| *t == chain.target) else {
            continue;
        };
        let target = chain.target as *mut core::ffi::c_void;
        // SAFETY: `target` is a function start the build table or reflection
        // vouches for; `code` is a thunk with the same calling convention.
        match unsafe { MinHook::create_hook(target, code.cast()) } {
            Ok(original) => {
                chain.original = original.addr();
                chain.wire(off);
                // SAFETY: the hook was just created and its slots are wired.
                if let Err(error) = unsafe { MinHook::enable_hook(target) } {
                    // SAFETY: undoes the hook created above.
                    let _removed = unsafe { MinHook::remove_hook(target) };
                    chain.head = None;
                    failures.push(Failure {
                        target: chain.target,
                        reason: format!("{error:?}"),
                    });
                }
            }
            Err(error) => {
                chain.head = None;
                failures.push(Failure {
                    target: chain.target,
                    reason: format!("{error:?}"),
                });
            }
        }
    }
    failures
}

/// Writes `[mov gs:[TlsSlots + index * 8], rsp]` and `jmp [rip + slot]`.
///
/// # Safety
///
/// `code` has `THUNK` writable bytes and `slot` lies within ±2 GiB of it.
unsafe fn write_thunk(code: *mut u8, slot: &AtomicUsize, frame_index: Option<u32>) {
    let mut bytes = [0xcc_u8; THUNK];
    let mut at = 0;
    if let Some(index) = frame_index {
        let displacement = TEB_TLS_SLOTS + index * 8;
        bytes[..5].copy_from_slice(&[0x65, 0x48, 0x89, 0x24, 0x25]);
        bytes[5..9].copy_from_slice(&displacement.to_le_bytes());
        at = 9;
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
        unsafe { write_thunk(code.as_mut_ptr(), &slot, Some(3)) };
        assert_eq!(code[..5], [0x65, 0x48, 0x89, 0x24, 0x25]);
        assert_eq!(
            u32::from_le_bytes(code[5..9].try_into().unwrap()),
            0x1480 + 24
        );
        assert_eq!(code[9..11], [0xff, 0x25]);
    }
}
