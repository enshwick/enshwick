//! The loader's own inline hooks: each target's first instructions become a
//! 5-byte jump to a relay near the target, the relay jumps on to the
//! detour, and a trampoline near the target runs the moved instructions and
//! jumps back. Hooks are installed in one batch and never removed; turning a
//! mod off rewires slots instead.
//!
//! Installing prepares every relay and trampoline first, seals their pages
//! executable, then suspends the other threads of the process for the few
//! writes to the targets, moving any thread that stood inside the replaced
//! bytes, and any return address into them on a thread's stack, into the
//! trampoline. Threads that live on fibers of their own are not seen: a
//! fiber that is not running keeps its stack where no thread points. The
//! game makes none: neither image imports or names a fiber call, and on the
//! server no thread left the stack its thread block records (build
//! 1269579); a fiber another module makes stays unseen.

use std::sync::Mutex;

use enshwick_loader_api::abi::Level;
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
    GetCurrentProcess, GetCurrentProcessId, GetCurrentThreadId, GetThreadId, OpenThread,
    ResumeThread, SuspendThread, THREAD_GET_CONTEXT, THREAD_QUERY_LIMITED_INFORMATION,
    THREAD_SET_CONTEXT, THREAD_SUSPEND_RESUME,
};

use crate::settings::LOADER_ID;

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
/// How often the writes wait for threads to leave the code being patched.
const SETTLE_TRIES: u32 = 50;
/// How long each wait lasts.
const SETTLE_WAIT: std::time::Duration = std::time::Duration::from_millis(2);
/// The most threads the writes suspend; the room is taken before any is.
const MAX_THREADS: usize = 4096;
/// `STATUS_NO_MORE_ENTRIES`: past the last thread.
const NO_MORE_ENTRIES: i32 = 0x8000_001a_u32.cast_signed();

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
///
/// A suspended thread may stand inside the bytes a hook replaces, or have
/// called out of them, with a return address into them on its stack; both
/// move to the same instruction in the trampoline. A code patch has no
/// trampoline, so the writes wait, a few times, until no thread stands or
/// returns inside one.
fn write_all(prepared: &[&Pending], code: &[Bytes<'_>]) -> bool {
    let writes: Vec<(usize, &[u8])> = prepared
        .iter()
        .map(|&p| (p.target, &p.patch[..]))
        .chain(code.iter().map(|c| (c.address, c.bytes)))
        .collect();
    if writes.is_empty() {
        return true;
    }
    let moved = moved_instructions(prepared);
    let patched: Vec<(usize, usize)> = code
        .iter()
        .map(|c| (c.address, c.address + c.bytes.len()))
        .collect();
    let returns = patch_returns(code);
    // Nothing below allocates while the other threads stand still: one of
    // them may hold the heap's lock.
    let mut protections = Vec::with_capacity(writes.len());
    let mut threads = Threads::new();
    let mut tries = 0;
    let unsettled = loop {
        if !threads.suspend() {
            threads.resume();
            return false;
        }
        let busy = !patched.is_empty()
            && threads
                .iter()
                .any(|thread| in_flight(thread, &patched, &returns));
        if !busy || tries == SETTLE_TRIES {
            break busy;
        }
        threads.resume();
        tries += 1;
        std::thread::sleep(SETTLE_WAIT);
    };
    let mut writable = true;
    for &(address, bytes) in &writes {
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
        for &(address, bytes) in &writes {
            // SAFETY: the target is writable now; a hook's first `stolen`
            // bytes were moved to the trampoline, and a code patch's bytes
            // were checked against what its owner expected.
            unsafe { (address as *mut u8).copy_from_nonoverlapping(bytes.as_ptr(), bytes.len()) };
        }
        for thread in threads.iter() {
            relocate(thread, &moved);
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
    threads.resume();
    if unsettled {
        crate::log::write(
            Level::WARN,
            LOADER_ID,
            "code patched while a thread still stood in or returned into it",
        );
    }
    writable
}

/// Where each instruction a hook moves stands in the target and in the
/// trampoline, by its address in the target, sorted; the first instruction
/// of each is left out, since the jump starts there.
fn moved_instructions(prepared: &[&Pending]) -> Vec<(usize, usize)> {
    let mut moved: Vec<(usize, usize)> = prepared
        .iter()
        .flat_map(|p| {
            p.moved
                .iter()
                .filter(|&&(old, _)| old != 0)
                .map(|&(old, new)| (p.target + old, p.trampoline + new))
        })
        .collect();
    moved.sort_unstable();
    moved
}

/// Addresses inside the code patches that follow a call there in the code
/// as it is now, sorted: where a thread that called out of a patch returns.
fn patch_returns(code: &[Bytes<'_>]) -> Vec<usize> {
    let mut returns: Vec<usize> = code
        .iter()
        .flat_map(|c| {
            (c.address + 1..c.address + c.bytes.len()).filter(|&at| follows_call(c.address, at))
        })
        .collect();
    returns.sort_unstable();
    returns.dedup();
    returns
}

/// Whether a call starting at or after `start` ends at `at`.
fn follows_call(start: usize, at: usize) -> bool {
    /// The longest call: `call qword [rip + disp32]` with a prefix.
    const LONGEST: usize = 7;
    let from = start.max(at.saturating_sub(LONGEST));
    // SAFETY: the bytes from the patch's start, which the patch's check read
    // inside the game's code section.
    let before = unsafe { core::slice::from_raw_parts(from as *const u8, at - from) };
    (2..=before.len()).any(|len| {
        let mut decoder = Decoder::with_ip(
            64,
            &before[before.len() - len..],
            (at - len) as u64,
            DecoderOptions::NONE,
        );
        let instruction = decoder.decode();
        instruction.len() == len
            && matches!(
                instruction.flow_control(),
                FlowControl::Call | FlowControl::IndirectCall
            )
    })
}

/// A suspended thread's stack pointer and instruction pointer, with the
/// context read for writing it back.
fn control(thread: HANDLE) -> Option<CONTEXT> {
    let mut context: CONTEXT = unsafe_zeroed_context();
    context.ContextFlags = CONTEXT_CONTROL_AMD64;
    // SAFETY: a suspended thread opened with context rights.
    (unsafe { GetThreadContext(thread, &raw mut context) } != 0).then_some(context)
}

/// The words on a suspended thread's stack from `rsp` up to its base.
fn stack(rsp: u64) -> Option<&'static mut [usize]> {
    let rsp = usize::try_from(rsp)
        .ok()?
        .next_multiple_of(size_of::<usize>());
    let mut info = MEMORY_BASIC_INFORMATION {
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
            rsp as *const _,
            &raw mut info,
            size_of::<MEMORY_BASIC_INFORMATION>(),
        )
    };
    if known == 0 || info.State != MEM_COMMIT || info.Protect != PAGE_READWRITE {
        return None;
    }
    let end = info.BaseAddress.addr() + info.RegionSize;
    let words = end.checked_sub(rsp)? / size_of::<usize>();
    // SAFETY: committed read-write pages of a thread that stands still, from
    // its stack pointer to the end of the region, the stack's base.
    Some(unsafe { core::slice::from_raw_parts_mut(rsp as *mut usize, words) })
}

/// Whether a suspended thread stands inside a patched range or holds a
/// return address into one on its stack.
fn in_flight(thread: HANDLE, patched: &[(usize, usize)], returns: &[usize]) -> bool {
    let Some(context) = control(thread) else {
        return false;
    };
    let rip = usize::try_from(context.Rip).unwrap_or(0);
    if patched.iter().any(|&(start, end)| start < rip && rip < end) {
        return true;
    }
    let (Some(&low), Some(&high)) = (returns.first(), returns.last()) else {
        return false;
    };
    stack(context.Rsp).is_some_and(|words| {
        words
            .iter()
            .any(|&word| (low..=high).contains(&word) && returns.binary_search(&word).is_ok())
    })
}

/// Moves a suspended thread standing in replaced bytes into the trampoline,
/// and the return addresses into them on its stack.
fn relocate(thread: HANDLE, moved: &[(usize, usize)]) {
    let (Some(&(low, _)), Some(&(high, _))) = (moved.first(), moved.last()) else {
        return;
    };
    let find = |address: usize| {
        (low..=high)
            .contains(&address)
            .then(|| moved.binary_search_by_key(&address, |&(old, _)| old).ok())
            .flatten()
            .map(|i| moved[i].1)
    };
    let Some(mut context) = control(thread) else {
        return;
    };
    if let Some(new) = find(usize::try_from(context.Rip).unwrap_or(0)) {
        context.Rip = new as u64;
        // SAFETY: as in `control`; the trampoline holds the same instruction.
        unsafe { SetThreadContext(thread, &raw const context) };
    }
    if let Some(words) = stack(context.Rsp) {
        for word in words {
            if let Some(new) = find(*word) {
                // A return address, or a stale copy of one: either way the
                // trampoline's instruction is the same one.
                *word = new;
            }
        }
    }
}

fn unsafe_zeroed_context() -> CONTEXT {
    // SAFETY: `CONTEXT` is plain data, valid when zeroed.
    unsafe { core::mem::zeroed() }
}

#[link(name = "ntdll", kind = "raw-dylib")]
unsafe extern "system" {
    /// The thread of `process` after `thread`, or its first for null.
    fn NtGetNextThread(
        process: HANDLE,
        thread: HANDLE,
        access: u32,
        attributes: u32,
        flags: u32,
        next: *mut HANDLE,
    ) -> i32;
}

/// The other threads of the process, suspended together.
struct Threads {
    held: Vec<(HANDLE, u32)>,
}

impl Threads {
    /// Room for the threads, taken before any is suspended.
    fn new() -> Self {
        Self {
            held: Vec::with_capacity(MAX_THREADS),
        }
    }

    /// Suspends every other thread, including those started meanwhile;
    /// false when there are more than [`MAX_THREADS`] or none can be listed.
    fn suspend(&mut self) -> bool {
        // SAFETY: no preconditions.
        let me = unsafe { GetCurrentThreadId() };
        // SAFETY: no preconditions.
        let process = unsafe { GetCurrentProcess() };
        let access = THREAD_SUSPEND_RESUME
            | THREAD_GET_CONTEXT
            | THREAD_SET_CONTEXT
            | THREAD_QUERY_LIMITED_INFORMATION;
        loop {
            let mut fresh = false;
            let mut cursor: HANDLE = core::ptr::null_mut();
            loop {
                let mut next: HANDLE = core::ptr::null_mut();
                // SAFETY: our own process; `cursor` is null or a handle from
                // the previous call.
                let status =
                    unsafe { NtGetNextThread(process, cursor, access, 0, 0, &raw mut next) };
                let kept = self.held.iter().any(|&(h, _)| h == cursor);
                if !cursor.is_null() && !kept {
                    // SAFETY: a handle from the previous call, not kept.
                    unsafe { CloseHandle(cursor) };
                }
                if status < 0 {
                    if self.held.is_empty() && status != NO_MORE_ENTRIES {
                        // A system without the call, such as an old Wine.
                        return self.suspend_listed(me, access);
                    }
                    break;
                }
                cursor = next;
                // SAFETY: a thread handle opened with query rights.
                let id = unsafe { GetThreadId(next) };
                if id == me || self.held.iter().any(|&(_, known)| known == id) {
                    continue;
                }
                if self.held.len() == self.held.capacity() {
                    // SAFETY: as above; the cursor is not kept.
                    unsafe { CloseHandle(next) };
                    return false;
                }
                // SAFETY: a thread of our own process, opened with suspend rights.
                if unsafe { SuspendThread(next) } != u32::MAX {
                    self.held.push((next, id));
                    fresh = true;
                }
            }
            if !fresh {
                return true;
            }
        }
    }

    /// Suspends the other threads a snapshot lists, once: a thread started
    /// meanwhile stays running.
    fn suspend_listed(&mut self, me: u32, access: u32) -> bool {
        // SAFETY: no preconditions.
        let process = unsafe { GetCurrentProcessId() };
        // SAFETY: a thread snapshot of the system.
        let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
        if snapshot == INVALID_HANDLE_VALUE {
            return false;
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
        // SAFETY: a valid snapshot and entry.
        let mut more = unsafe { Thread32First(snapshot, &raw mut entry) } != 0;
        while more && self.held.len() < self.held.capacity() {
            let id = entry.th32ThreadID;
            if entry.th32OwnerProcessID == process && id != me {
                // SAFETY: opening a thread of our own process.
                let thread = unsafe { OpenThread(access, 0, id) };
                // SAFETY: a thread of our own process, opened with suspend
                // rights; a null handle fails the call.
                if !thread.is_null() && unsafe { SuspendThread(thread) } != u32::MAX {
                    self.held.push((thread, id));
                } else if !thread.is_null() {
                    // SAFETY: opened above, not kept.
                    unsafe { CloseHandle(thread) };
                }
            }
            // SAFETY: as above.
            more = unsafe { Thread32Next(snapshot, &raw mut entry) } != 0;
        }
        // SAFETY: the snapshot is ours.
        unsafe { CloseHandle(snapshot) };
        !more
    }

    /// The suspended threads.
    fn iter(&self) -> impl Iterator<Item = HANDLE> + '_ {
        self.held.iter().map(|&(handle, _)| handle)
    }

    /// Resumes and lets go of every thread.
    fn resume(&mut self) {
        for (thread, _) in self.held.drain(..) {
            // SAFETY: suspended in `suspend`.
            unsafe { ResumeThread(thread) };
            // SAFETY: opened in `suspend`.
            unsafe { CloseHandle(thread) };
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

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

    /// Sets `flags[1]`, then waits for `flags[0]` and returns; `r8` points at
    /// the flags. Touches no stack, so any alignment does.
    const SPIN: [u8; 16] = [
        0x41, 0xc7, 0x40, 0x04, 0x01, 0x00, 0x00, 0x00, // mov dword [r8 + 4], 1
        0x41, 0x8b, 0x00, // mov eax, [r8]
        0x85, 0xc0, // test eax, eax
        0x74, 0xf9, // jz -7
        0xc3, // ret
    ];

    type Calling = extern "system" fn(u32, usize, *const AtomicU32) -> u32;

    /// A thread running `entry(5, spin, flags)`, once it waits inside the
    /// spin.
    fn waiting_in(entry: usize, flags: &'static [AtomicU32; 2]) -> std::thread::JoinHandle<u32> {
        let spin = function(&SPIN);
        // SAFETY: each caller's code takes these arguments.
        let call = unsafe { core::mem::transmute::<usize, Calling>(entry) };
        let thread = std::thread::spawn(move || call(5, spin, flags.as_ptr()));
        while flags[1].load(Ordering::Acquire) == 0 {
            std::thread::yield_now();
        }
        thread
    }

    #[test]
    fn a_thread_that_called_out_of_the_hooked_bytes_returns_into_the_trampoline() {
        static FLAGS: [AtomicU32; 2] = [AtomicU32::new(0), AtomicU32::new(0)];
        let target = function(&[
            0xff, 0xd2, // call rdx <- returns inside the jump
            0x90, 0x90, 0x90, // nop
            0x8d, 0x41, 0x07, // lea eax, [rcx + 7]
            0xc3, // ret
            0xcc, 0xcc, 0xcc,
        ]);
        let thread = waiting_in(target, &FLAGS);
        let original = hook(target).unwrap();
        FLAGS[0].store(1, Ordering::Release);
        assert_eq!(thread.join().unwrap(), 12);
        assert_eq!(callable(target)(5), 105);
        assert_ne!(original, target);
    }

    #[test]
    fn a_code_patch_waits_for_a_thread_that_returns_into_it() {
        static FLAGS: [AtomicU32; 2] = [AtomicU32::new(0), AtomicU32::new(0)];
        let target = function(&[
            0xff, 0xd2, // call rdx <- returns inside the patch
            0xb8, 0x01, 0x00, 0x00, 0x00, // mov eax, 1
            0xc3, // ret
            0xcc, 0xcc,
        ]);
        let thread = waiting_in(target, &FLAGS);
        let release = std::thread::spawn(|| {
            std::thread::sleep(std::time::Duration::from_millis(30));
            FLAGS[0].store(1, Ordering::Release);
        });
        let patch = [Bytes {
            address: target,
            // mov eax, 2; nop; nop
            bytes: &[0xb8, 0x02, 0x00, 0x00, 0x00, 0x90, 0x90],
        }];
        assert!(commit(&[], &patch));
        assert_eq!(thread.join().unwrap(), 1);
        release.join().unwrap();
        assert_eq!(callable(target)(0), 2);
    }

    #[test]
    fn only_instructions_after_the_first_move_and_only_returns_after_calls_count() {
        let pending = Pending {
            original: 0x9000,
            target: 0x1000,
            patch: [0; PATCH],
            stolen: 6,
            trampoline: 0x9000,
            moved: vec![(0, 0), (2, 2), (4, 7)],
        };
        assert_eq!(
            moved_instructions(&[&pending]),
            [(0x1002, 0x9002), (0x1004, 0x9007)]
        );
        // call rdx; mov eax, 1; ret
        let code = function(&[0xff, 0xd2, 0xb8, 0x01, 0x00, 0x00, 0x00, 0xc3]);
        let bytes = [0; 7];
        let patch = [Bytes {
            address: code,
            bytes: &bytes,
        }];
        assert_eq!(patch_returns(&patch), [code + 2]);
    }

    /// Whether a thread counting on its own stands still while `threads`
    /// holds it, and counts again once they are resumed.
    fn stops_and_goes(suspend: impl FnOnce(&mut Threads) -> bool) -> bool {
        let count = std::sync::Arc::new(AtomicU32::new(0));
        let stop = std::sync::Arc::new(AtomicU32::new(0));
        let counting = {
            let (count, stop) = (std::sync::Arc::clone(&count), std::sync::Arc::clone(&stop));
            std::thread::spawn(move || {
                while stop.load(Ordering::Relaxed) == 0 {
                    count.fetch_add(1, Ordering::Relaxed);
                }
            })
        };
        while count.load(Ordering::Relaxed) == 0 {
            std::thread::yield_now();
        }
        let mut threads = Threads::new();
        let suspended = suspend(&mut threads);
        let before = count.load(Ordering::Relaxed);
        std::thread::sleep(std::time::Duration::from_millis(20));
        let still = count.load(Ordering::Relaxed) == before;
        threads.resume();
        std::thread::sleep(std::time::Duration::from_millis(20));
        let moves = count.load(Ordering::Relaxed) != before;
        stop.store(1, Ordering::Relaxed);
        counting.join().unwrap();
        suspended && still && moves
    }

    #[test]
    fn every_other_thread_stands_still_until_resumed() {
        assert!(stops_and_goes(Threads::suspend));
    }

    #[test]
    fn a_snapshot_suspends_the_other_threads_where_the_system_has_no_thread_walk() {
        assert!(stops_and_goes(|threads| {
            // SAFETY: no preconditions.
            let me = unsafe { GetCurrentThreadId() };
            threads.suspend_listed(me, THREAD_SUSPEND_RESUME)
        }));
    }
}
