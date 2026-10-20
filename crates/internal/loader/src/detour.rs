//! The loader's own inline hooks: each target's first instructions become a
//! 5-byte jump to a relay near the target, the relay jumps on to the
//! detour, and a trampoline near the target runs the moved instructions and
//! jumps back. Hooks are installed in one batch and never removed; turning a
//! mod off rewires slots instead.
//!
//! Installing prepares every relay and trampoline first, seals their pages
//! executable, then suspends the other threads of the process for the few
//! writes to the targets, moving any thread that stood inside the replaced
//! bytes into the trampoline.

use std::sync::Mutex;

use iced_x86::{
    BlockEncoder, BlockEncoderOptions, Decoder, DecoderOptions, FlowControl, Instruction,
    InstructionBlock,
};
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Diagnostics::Debug::{
    CONTEXT, CONTEXT_CONTROL_AMD64, FlushInstructionCache, GetThreadContext, SetThreadContext,
};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
};
use windows_sys::Win32::System::Memory::{
    MEM_COMMIT, MEM_FREE, MEM_RESERVE, MEMORY_BASIC_INFORMATION, PAGE_EXECUTE_READ,
    PAGE_EXECUTE_READWRITE, PAGE_READWRITE, VirtualAlloc, VirtualProtect, VirtualQuery,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentProcessId, GetCurrentThreadId, OpenThread, ResumeThread,
    SuspendThread, THREAD_GET_CONTEXT, THREAD_SET_CONTEXT, THREAD_SUSPEND_RESUME,
};

/// The jump written over a target: `jmp rel32`.
const PATCH: usize = 5;
/// `jmp [rip + 0]` followed by the 64-bit destination.
const FAR_JUMP: usize = 14;
/// Bytes read from a target to find the instructions to move.
const READ: usize = 32;
/// A relay and a trampoline: moved instructions grow at most to this.
const ENTRY: usize = 128;
/// Windows reserves memory in 64 KiB steps.
const GRANULARITY: usize = 0x1_0000;
const BLOCK: usize = GRANULARITY;
/// How far a `rel32` reaches, kept clear of the edge for the block's length.
const REACH: usize = 0x7fff_0000 - BLOCK;

/// A hook to install: the function and where its callers should go instead.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Request {
    pub(crate) target: usize,
    pub(crate) detour: usize,
    /// The target is an instruction inside a function, not its start.
    pub(crate) inside: bool,
}

/// A prepared hook: the bytes to write over the target and where the moved
/// instructions went.
#[derive(Debug)]
pub(crate) struct Pending {
    /// Calls the original function: the moved instructions, then the rest.
    pub(crate) original: usize,
    target: usize,
    patch: [u8; PATCH],
    stolen: usize,
    trampoline: usize,
    /// Offsets of the moved instructions in the target and in the trampoline.
    moved: Vec<(usize, usize)>,
}

/// Blocks of near memory, each written once and then sealed.
static BLOCKS: Mutex<Vec<(usize, usize)>> = Mutex::new(Vec::new());

/// Prepares every request: relays and trampolines written and sealed, the
/// targets untouched. Each result carries the trampoline that calls the
/// original function, or why the target is left alone.
pub(crate) fn prepare(requests: &[Request]) -> Vec<Result<Pending, String>> {
    let mut fresh: Vec<usize> = Vec::new();
    let prepared: Vec<Result<Pending, String>> = requests
        .iter()
        .map(|request| prepare_one(*request, &mut fresh))
        .collect();
    for &block in &fresh {
        let mut old = 0;
        // SAFETY: a block this module allocated, fully written.
        let sealed =
            unsafe { VirtualProtect(block as *const _, BLOCK, PAGE_EXECUTE_READ, &raw mut old) };
        if sealed == 0 {
            return requests
                .iter()
                .map(|_| Err("relay pages not executable".to_owned()))
                .collect();
        }
    }
    prepared
}

/// Bytes to write over game code in the same batch as the hooks.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Bytes<'a> {
    /// Where they go.
    pub(crate) address: usize,
    /// What goes there.
    pub(crate) bytes: &'a [u8],
}

impl Pending {
    /// The bytes of the target the hook replaces or moves: start and length.
    pub(crate) fn span(&self) -> (usize, usize) {
        (self.target, self.stolen)
    }
}

/// Writes the jumps over the targets of `pending` and the `code` patches,
/// all or none; false when none could be written.
pub(crate) fn commit(pending: &[&Pending], code: &[Bytes<'_>]) -> bool {
    write_all(pending, code)
}

fn prepare_one(request: Request, fresh: &mut Vec<usize>) -> Result<Pending, String> {
    // SAFETY: the build table, reflection or a mod vouches for an instruction
    // start, followed by at least `READ` mapped bytes of code or padding.
    let code = unsafe { core::slice::from_raw_parts(request.target as *const u8, READ) };
    let mut decoder = Decoder::with_ip(64, code, request.target as u64, DecoderOptions::NONE);
    let mut instructions: Vec<Instruction> = Vec::new();
    let mut stolen = 0;
    while stolen < PATCH {
        let instruction = decoder.decode();
        if instruction.is_invalid() {
            return Err("its first instructions do not decode".into());
        }
        // Bytes after a return or a jump away belong to other code, which
        // the jump must not overwrite.
        match instruction.flow_control() {
            FlowControl::Return | FlowControl::Interrupt | FlowControl::Exception
                if !request.inside && stolen + instruction.len() < PATCH =>
            {
                return Err("the function is shorter than the jump".into());
            }
            FlowControl::Return
            | FlowControl::Interrupt
            | FlowControl::Exception
            | FlowControl::UnconditionalBranch
            | FlowControl::IndirectBranch
                if stolen + instruction.len() < PATCH =>
            {
                return Err("control leaves within the bytes of the jump".into());
            }
            _ => {}
        }
        stolen += instruction.len();
        instructions.push(instruction);
    }
    let entry = near(request.target, fresh)?;
    let (relay, trampoline) = (entry, entry + FAR_JUMP);
    let block = InstructionBlock::new(&instructions, trampoline as u64);
    let encoded = BlockEncoder::encode(
        64,
        block,
        BlockEncoderOptions::RETURN_NEW_INSTRUCTION_OFFSETS,
    )
    .map_err(|e| format!("its first instructions cannot move: {e}"))?;
    let back = request.target + stolen;
    if FAR_JUMP + encoded.code_buffer.len() + FAR_JUMP > ENTRY {
        return Err("its first instructions grow too long to move".into());
    }
    let mut bytes = Vec::with_capacity(ENTRY);
    bytes.extend_from_slice(&far_jump(request.detour));
    bytes.extend_from_slice(&encoded.code_buffer);
    bytes.extend_from_slice(&far_jump(back));
    // SAFETY: `entry` has `ENTRY` writable bytes in a block not yet sealed.
    unsafe { (entry as *mut u8).copy_from_nonoverlapping(bytes.as_ptr(), bytes.len()) };
    let mut offset = 0;
    let moved = instructions
        .iter()
        .zip(&encoded.new_instruction_offsets)
        .map(|(instruction, &new)| {
            let pair = (offset, new as usize);
            offset += instruction.len();
            pair
        })
        .collect();
    let distance = i32::try_from(relay.wrapping_sub(request.target + PATCH).cast_signed())
        .map_err(|_| "no relay within reach".to_owned())?;
    let mut patch = [0xe9, 0, 0, 0, 0];
    patch[1..].copy_from_slice(&distance.to_le_bytes());
    Ok(Pending {
        original: trampoline,
        target: request.target,
        patch,
        stolen,
        trampoline,
        moved,
    })
}

/// `jmp [rip + 0]` to `destination`.
fn far_jump(destination: usize) -> [u8; FAR_JUMP] {
    let mut bytes = [0xff, 0x25, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    bytes[6..].copy_from_slice(&(destination as u64).to_le_bytes());
    bytes
}

/// `ENTRY` bytes within `rel32` reach of `target`, from a block allocated in
/// this batch.
fn near(target: usize, fresh: &mut Vec<usize>) -> Result<usize, String> {
    let mut blocks = BLOCKS
        .lock()
        .map_err(|_| "the relay allocator is poisoned".to_owned())?;
    let reaches = |base: usize| base.abs_diff(target) < REACH;
    if let Some((base, used)) = blocks
        .iter_mut()
        .find(|(base, used)| fresh.contains(base) && reaches(*base) && *used + ENTRY <= BLOCK)
    {
        let entry = *base + *used;
        *used += ENTRY;
        return Ok(entry);
    }
    let base = allocate_near(target).ok_or("no free memory within reach of the target")?;
    fresh.push(base);
    blocks.push((base, ENTRY));
    Ok(base)
}

/// Reserves and commits a block within reach of `target`, nearest first.
fn allocate_near(target: usize) -> Option<usize> {
    let start = target & !(GRANULARITY - 1);
    let low = start.saturating_sub(REACH);
    let high = start.saturating_add(REACH);
    let below = (low..start).rev().step_by(GRANULARITY);
    let above = (start + GRANULARITY..high).step_by(GRANULARITY);
    below
        .zip(above)
        .flat_map(|(a, b)| [a, b])
        .find_map(|address| {
            let mut info: MEMORY_BASIC_INFORMATION = MEMORY_BASIC_INFORMATION {
                BaseAddress: core::ptr::null_mut(),
                AllocationBase: core::ptr::null_mut(),
                AllocationProtect: 0,
                PartitionId: 0,
                RegionSize: 0,
                State: 0,
                Protect: 0,
                Type: 0,
            };
            // SAFETY: querying any address is allowed; the struct is writable.
            let known = unsafe {
                VirtualQuery(
                    address as *const _,
                    &raw mut info,
                    size_of::<MEMORY_BASIC_INFORMATION>(),
                )
            };
            if known == 0 || info.State != MEM_FREE {
                return None;
            }
            // SAFETY: a free region; null when it was taken meanwhile.
            let block = unsafe {
                VirtualAlloc(
                    address as *const _,
                    BLOCK,
                    MEM_RESERVE | MEM_COMMIT,
                    PAGE_READWRITE,
                )
            };
            (!block.is_null()).then(|| block.addr())
        })
}

/// Writes every hook jump and code patch with the other threads suspended;
/// false when any target could not be made writable, in which case none is
/// written.
fn write_all(prepared: &[&Pending], code: &[Bytes<'_>]) -> bool {
    let writes: Vec<(usize, &[u8], Option<&Pending>)> = prepared
        .iter()
        .map(|&p| (p.target, &p.patch[..], Some(p)))
        .chain(code.iter().map(|c| (c.address, c.bytes, None)))
        .collect();
    if writes.is_empty() {
        return true;
    }
    // Nothing below allocates while the other threads stand still: one of
    // them may hold the heap's lock.
    let mut protections = Vec::with_capacity(writes.len());
    let threads = other_threads();
    for &thread in &threads {
        // SAFETY: a thread handle opened with suspend rights.
        unsafe { SuspendThread(thread) };
    }
    let mut writable = true;
    for &(address, bytes, _) in &writes {
        let mut old = 0;
        // SAFETY: the target's code pages; restored below.
        let ok = unsafe {
            VirtualProtect(
                address as *const _,
                bytes.len(),
                PAGE_EXECUTE_READWRITE,
                &raw mut old,
            )
        };
        writable &= ok != 0;
        protections.push((address, bytes.len(), old, ok != 0));
    }
    if writable {
        for &(address, bytes, hook) in &writes {
            // SAFETY: the target is writable now; a hook's first `stolen`
            // bytes were moved to the trampoline, and a code patch's bytes
            // were checked against what its owner expected.
            unsafe { (address as *mut u8).copy_from_nonoverlapping(bytes.as_ptr(), bytes.len()) };
            if let Some(p) = hook {
                for &thread in &threads {
                    relocate(thread, p);
                }
            }
        }
    }
    // SAFETY: no preconditions.
    let process = unsafe { GetCurrentProcess() };
    for (address, len, old, changed) in protections {
        if changed {
            let mut ignored = 0;
            // SAFETY: restores what was there before.
            unsafe { VirtualProtect(address as *const _, len, old, &raw mut ignored) };
            // SAFETY: our own process's code.
            unsafe { FlushInstructionCache(process, address as *const _, len) };
        }
    }
    for &thread in &threads {
        // SAFETY: suspended above.
        unsafe { ResumeThread(thread) };
        // SAFETY: opened in `other_threads`.
        unsafe { CloseHandle(thread) };
    }
    writable
}

/// Moves a suspended thread standing in the replaced bytes into the trampoline.
fn relocate(thread: HANDLE, p: &Pending) {
    let mut context: CONTEXT = unsafe_zeroed_context();
    context.ContextFlags = CONTEXT_CONTROL_AMD64;
    // SAFETY: a suspended thread opened with context rights.
    if unsafe { GetThreadContext(thread, &raw mut context) } == 0 {
        return;
    }
    let rip = usize::try_from(context.Rip).unwrap_or(0);
    let Some(offset) = rip.checked_sub(p.target).filter(|&o| o < p.stolen) else {
        return;
    };
    if let Some(&(_, new)) = p.moved.iter().find(|&&(old, _)| old == offset) {
        context.Rip = (p.trampoline + new) as u64;
        // SAFETY: as above; the trampoline holds the same instruction.
        unsafe { SetThreadContext(thread, &raw const context) };
    }
}

fn unsafe_zeroed_context() -> CONTEXT {
    // SAFETY: `CONTEXT` is plain data, valid when zeroed.
    unsafe { core::mem::zeroed() }
}

/// Handles to every other thread of the process, with suspend and context
/// rights.
fn other_threads() -> Vec<HANDLE> {
    // SAFETY: no preconditions.
    let process = unsafe { GetCurrentProcessId() };
    // SAFETY: no preconditions.
    let me = unsafe { GetCurrentThreadId() };
    // SAFETY: a thread snapshot of the system.
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return Vec::new();
    }
    let mut entry = THREADENTRY32 {
        dwSize: u32::try_from(size_of::<THREADENTRY32>()).unwrap_or(0),
        cntUsage: 0,
        th32ThreadID: 0,
        th32OwnerProcessID: 0,
        tpBasePri: 0,
        tpDeltaPri: 0,
        dwFlags: 0,
    };
    let mut threads = Vec::new();
    // SAFETY: a valid snapshot and entry.
    let mut more = unsafe { Thread32First(snapshot, &raw mut entry) } != 0;
    while more {
        if entry.th32OwnerProcessID == process && entry.th32ThreadID != me {
            // SAFETY: opening a thread of our own process.
            let thread = unsafe {
                OpenThread(
                    THREAD_SUSPEND_RESUME | THREAD_GET_CONTEXT | THREAD_SET_CONTEXT,
                    0,
                    entry.th32ThreadID,
                )
            };
            if !thread.is_null() {
                threads.push(thread);
            }
        }
        // SAFETY: as above.
        more = unsafe { Thread32Next(snapshot, &raw mut entry) } != 0;
    }
    // SAFETY: the snapshot is ours.
    unsafe { CloseHandle(snapshot) };
    threads
}

#[cfg(test)]
mod tests {
    use super::*;

    type Unary = extern "system" fn(u32) -> u32;

    /// Writes `code` to fresh executable memory and returns its address.
    fn function(code: &[u8]) -> usize {
        // SAFETY: a fresh allocation, written before it is executed.
        let page = unsafe {
            VirtualAlloc(
                core::ptr::null(),
                4096,
                MEM_RESERVE | MEM_COMMIT,
                PAGE_EXECUTE_READWRITE,
            )
        };
        assert!(!page.is_null());
        // SAFETY: 4096 writable bytes.
        unsafe {
            page.cast::<u8>()
                .copy_from_nonoverlapping(code.as_ptr(), code.len());
        }
        page.addr()
    }

    extern "system" fn plus_hundred(value: u32) -> u32 {
        value + 100
    }

    fn callable(address: usize) -> Unary {
        // SAFETY: every test function takes a u32 in ecx and returns one in eax.
        unsafe { core::mem::transmute::<usize, Unary>(address) }
    }

    /// Hooks `target` to `plus_hundred`; the trampoline or why not.
    fn hook(target: usize) -> Result<usize, String> {
        let pending = prepare(&[Request {
            target,
            detour: (plus_hundred as Unary as *const ()).addr(),
            inside: false,
        }]);
        let ready: Vec<&Pending> = pending.iter().filter_map(|p| p.as_ref().ok()).collect();
        assert!(commit(&ready, &[]));
        pending.into_iter().next().unwrap().map(|p| p.original)
    }

    #[test]
    fn a_hooked_function_reaches_the_detour_and_the_trampoline_the_original() {
        // mov eax, ecx; add eax, 1; ret; padding
        let target = function(&[0x89, 0xc8, 0x83, 0xc0, 0x01, 0xc3, 0xcc, 0xcc, 0xcc]);
        let original = hook(target).unwrap();
        assert_eq!(callable(target)(1), 101);
        assert_eq!(callable(original)(1), 2);
    }

    #[test]
    fn a_rip_relative_operand_still_points_at_its_data() {
        let code = [
            0x48, 0x8d, 0x05, 0x0e, 0x00, 0x00, 0x00, // lea rax, [rip + 14] -> offset 21
            0x8b, 0x00, // mov eax, [rax]
            0xc3, // ret
            0xcc, 0xcc, 0xcc, 0xcc, 0xcc, 0xcc, 0xcc, 0xcc, 0xcc, 0xcc, 0xcc, // padding
            0x2a, 0x00, 0x00, 0x00, // 42
        ];
        let original = hook(function(&code)).unwrap();
        assert_eq!(callable(original)(0), 42);
    }

    #[test]
    fn a_moved_short_branch_still_lands_in_the_original() {
        let code = [
            0x85, 0xc9, // test ecx, ecx
            0x74, 0x06, // jz +6 -> offset 10
            0xb8, 0x05, 0x00, 0x00, 0x00, // mov eax, 5
            0xc3, // ret
            0xb8, 0x09, 0x00, 0x00, 0x00, // mov eax, 9
            0xc3, // ret
        ];
        let original = hook(function(&code)).unwrap();
        assert_eq!(callable(original)(1), 5);
        assert_eq!(callable(original)(0), 9);
    }

    #[test]
    fn code_patches_are_written_in_the_batch_with_the_hooks() {
        // mov eax, 1; ret; padding
        let patched = function(&[0xb8, 0x01, 0x00, 0x00, 0x00, 0xc3, 0xcc, 0xcc]);
        let hooked = function(&[0x89, 0xc8, 0x83, 0xc0, 0x01, 0xc3, 0xcc, 0xcc, 0xcc]);
        let pending = prepare(&[Request {
            target: hooked,
            detour: (plus_hundred as Unary as *const ()).addr(),
            inside: false,
        }]);
        let ready: Vec<&Pending> = pending.iter().filter_map(|p| p.as_ref().ok()).collect();
        assert_eq!(ready[0].span(), (hooked, 5));
        let code = [Bytes {
            address: patched + 1,
            bytes: &[0x02],
        }];
        assert!(commit(&ready, &code));
        assert_eq!(callable(patched)(0), 2);
        assert_eq!(callable(hooked)(1), 101);
    }

    #[test]
    fn a_function_that_jumps_away_within_the_jump_is_refused() {
        // jmp +3; the next function's first bytes, which the jump would cover
        let target = function(&[0xeb, 0x03, 0xb8, 0x01, 0x00, 0x00, 0x00, 0xc3, 0xcc]);
        assert!(hook(target).is_err());
        // jmp rel32: the whole jump moves, nothing past it is touched
        let thunk = function(&[0xe9, 0x00, 0x00, 0x00, 0x00, 0x89, 0xc8, 0xc3, 0xcc]);
        let original = hook(thunk).unwrap();
        assert_eq!(callable(original)(4), 4);
    }

    #[test]
    fn a_function_shorter_than_the_jump_is_refused() {
        // ret; int3...
        let target = function(&[0xc3, 0xcc, 0xcc, 0xcc, 0xcc, 0xcc, 0xcc, 0xcc]);
        assert!(hook(target).is_err());
    }
}
