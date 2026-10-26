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

use enshwick::abi::Level;
use iced_x86::{
    BlockEncoder, BlockEncoderOptions, Decoder, DecoderOptions, FlowControl, Instruction,
    InstructionBlock,
};
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Diagnostics::Debug::{
    CONTEXT, CONTEXT_CONTROL_AMD64, GetThreadContext, SetThreadContext,
};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
};
use windows_sys::Win32::System::Memory::{
    MEM_COMMIT, MEM_RESERVE, PAGE_EXECUTE_READWRITE, PAGE_READWRITE, VirtualAlloc,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentProcessId, GetCurrentThreadId, GetThreadId, OpenThread,
    ResumeThread, SuspendThread, THREAD_GET_CONTEXT, THREAD_QUERY_LIMITED_INFORMATION,
    THREAD_SET_CONTEXT, THREAD_SUSPEND_RESUME,
};

use crate::pages;
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
        // SAFETY: `block` is a block `allocate_near` reserved for this batch,
        // fully written above and not yet run by any thread.
        let sealed = unsafe { pages::seal(block, BLOCK) };
        if !sealed {
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

/// Whether `address` is mapped code followed by `len` bytes that read: a
/// wrong address from a mod or a table must refuse its hook, not fault.
fn hookable(address: usize, len: usize) -> bool {
    let Some(end) = address.checked_add(len) else {
        return false;
    };
    let Some(first) = pages::query(address).filter(pages::Region::runs) else {
        return false;
    };
    let mut reached = first.end;
    while reached < end {
        match pages::query(reached) {
            Some(next) if next.reads() && next.end > reached => reached = next.end,
            _ => return false,
        }
    }
    true
}

fn prepare_one(request: Request, fresh: &mut Vec<usize>) -> Result<Pending, String> {
    if !hookable(request.target, READ) {
        return Err("its address is not mapped code".into());
    }
    // SAFETY: `hookable` checked that `READ` bytes from the target lie in
    // committed, readable pages, the first of them code; the slice lives for
    // this function only, while no patch writes the target.
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
            if !pages::query(address).is_some_and(|found| found.is_free()) {
                return None;
            }
            // SAFETY: `VirtualAlloc` at a region `query` found free reserves
            // fresh pages or returns null when another thread took it
            // meanwhile; it touches no existing memory.
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
                // SAFETY: `suspend` just suspended every other thread of the
                // process, `iter` yields each of them, and they stay suspended
                // until `resume`.
                .any(|thread| unsafe { in_flight(thread, &patched, &returns) });
        if !busy || tries == SETTLE_TRIES {
            break busy;
        }
        threads.resume();
        tries += 1;
        std::thread::sleep(SETTLE_WAIT);
    };
    let mut writable = true;
    for &(address, bytes) in &writes {
        // SAFETY: the target's code pages, with every other thread of the
        // process suspended; the old protection is restored below before any
        // of them resumes.
        let old = unsafe { pages::protect(address, bytes.len(), PAGE_EXECUTE_READWRITE) };
        writable &= old.is_some();
        protections.push((address, bytes.len(), old));
    }
    if writable {
        for &(address, bytes) in &writes {
            // SAFETY: the target is writable now; a hook's first `stolen`
            // bytes were moved to the trampoline, and a code patch's bytes
            // were checked against what its owner expected.
            unsafe { (address as *mut u8).copy_from_nonoverlapping(bytes.as_ptr(), bytes.len()) };
        }
        for thread in threads.iter() {
            // SAFETY: every other thread of the process stays suspended from
            // `suspend` until `resume`, `iter` yields each of them, and
            // `moved` pairs each moved instruction with its trampoline copy.
            unsafe { relocate(thread, &moved) };
        }
    }
    // Backwards: two writes on one page saw the first one's protection, so
    // the first one's `old` is the page's own and must come last.
    for (address, len, old) in protections.into_iter().rev() {
        if let Some(old) = old {
            // SAFETY: the target's code pages, every other thread still
            // suspended, back to the protection they had before the write.
            let _restored = unsafe { pages::protect(address, len, old) };
            pages::flush(address, len);
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
    // SAFETY: `GetThreadContext` writes at most one `CONTEXT` into
    // `context`, a local with `ContextFlags` set; a handle without context
    // rights fails the call.
    (unsafe { GetThreadContext(thread, &raw mut context) } != 0).then_some(context)
}

/// The words on a suspended thread's stack, from its stack pointer up to
/// the stack's base, walked by address: no reference into another thread's
/// stack is made, and a walk lasts only for the call that takes it.
#[derive(Clone, Copy, Debug)]
struct Stack {
    /// The first word's address, aligned to a word.
    first: usize,
    /// The words up to the end of the stack's region.
    words: usize,
}

impl Stack {
    /// The stack that `rsp` points into: the committed read-write region
    /// from `rsp`, rounded up to a word, to the region's end.
    fn at(rsp: u64) -> Option<Self> {
        let first = usize::try_from(rsp)
            .ok()?
            .next_multiple_of(size_of::<usize>());
        let region = pages::query(first).filter(pages::Region::is_read_write)?;
        let words = region.end.checked_sub(first)? / size_of::<usize>();
        Some(Self { first, words })
    }

    fn address(self, word: usize) -> usize {
        self.first + word * size_of::<usize>()
    }

    /// Whether any word satisfies `test`.
    ///
    /// # Safety
    ///
    /// - Every thread of the process but the caller stays suspended for the
    ///   call, as `Threads::suspend` leaves them, so the stack's region stays
    ///   committed and no other thread writes it.
    unsafe fn any(self, mut test: impl FnMut(usize) -> bool) -> bool {
        (0..self.words).any(|word| {
            // SAFETY: `word` is below `words`, so its eight bytes lie in the
            // committed read-write region `at` found, which stays committed
            // and unwritten while every other thread stays suspended.
            test(unsafe { crate::memory::read(self.address(word), 0) })
        })
    }

    /// Replaces each word for which `new` gives a value.
    ///
    /// # Safety
    ///
    /// - Every thread of the process but the caller stays suspended for the
    ///   call, as `Threads::suspend` leaves them, so the stack's region stays
    ///   committed and no other thread reads or writes it.
    /// - Every value `new` gives is one the thread may find in that word.
    unsafe fn rewrite(self, mut new: impl FnMut(usize) -> Option<usize>) {
        for word in 0..self.words {
            let at = self.address(word);
            // SAFETY: `word` is below `words`, so its eight bytes lie in the
            // committed read-write region `at` found, unwritten while every
            // other thread stays suspended.
            let old: usize = unsafe { crate::memory::read(at, 0) };
            if let Some(value) = new(old) {
                // SAFETY: the same eight bytes, writable and touched by no
                // other thread while every other thread stays suspended; the
                // caller vouches for `value`.
                unsafe { crate::memory::write(at, 0, value) };
            }
        }
    }
}

/// Whether a suspended thread stands inside a patched range or holds a
/// return address into one on its stack.
///
/// # Safety
///
/// - Every thread of the process but the caller, `thread` among them, stays
///   suspended for the call, as `Threads::suspend` leaves them.
unsafe fn in_flight(thread: HANDLE, patched: &[(usize, usize)], returns: &[usize]) -> bool {
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
    Stack::at(context.Rsp).is_some_and(|stack| {
        // SAFETY: the caller keeps every other thread, the stack's owner
        // among them, suspended for the call.
        unsafe {
            stack.any(|word| (low..=high).contains(&word) && returns.binary_search(&word).is_ok())
        }
    })
}

/// Moves a suspended thread standing in replaced bytes into the trampoline,
/// and the return addresses into them on its stack.
///
/// # Safety
///
/// - Every thread of the process but the caller, `thread` among them, stays
///   suspended for the call, as `Threads::suspend` leaves them.
/// - Each moved address maps to the trampoline's copy of the same
///   instruction.
unsafe fn relocate(thread: HANDLE, moved: &[(usize, usize)]) {
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
        // SAFETY: the caller keeps `thread` suspended, `context` is the one
        // `control` read from it with only `Rip` changed, and the moved
        // address holds the same instruction in the trampoline.
        unsafe { SetThreadContext(thread, &raw const context) };
    }
    if let Some(stack) = Stack::at(context.Rsp) {
        // SAFETY: the caller keeps every other thread, the stack's owner
        // among them, suspended for the call and vouches for the moved
        // addresses. A word equal to a moved address is a return address, or
        // a stale copy of one: either way the trampoline holds the same
        // instruction.
        unsafe { stack.rewrite(find) };
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
        // SAFETY: `GetCurrentThreadId` takes no arguments and only reads
        // the calling thread's id.
        let me = unsafe { GetCurrentThreadId() };
        // SAFETY: `GetCurrentProcess` takes no arguments and returns a
        // pseudo-handle that needs no closing.
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
                // SAFETY: `process` is this process's pseudo-handle, `cursor`
                // is null or the handle the previous call returned and is still
                // open, and `next` is a local the call writes.
                let status =
                    unsafe { NtGetNextThread(process, cursor, access, 0, 0, &raw mut next) };
                let kept = self.held.iter().any(|&(h, _)| h == cursor);
                if !cursor.is_null() && !kept {
                    // SAFETY: `cursor` is an open handle the previous call
                    // returned, which `held` does not keep, closed once here.
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
                // SAFETY: `next` is an open thread handle `NtGetNextThread`
                // returned with `THREAD_QUERY_LIMITED_INFORMATION`.
                let id = unsafe { GetThreadId(next) };
                if id == me || self.held.iter().any(|&(_, known)| known == id) {
                    continue;
                }
                if self.held.len() == self.held.capacity() {
                    // SAFETY: `next` is an open handle `NtGetNextThread`
                    // returned, which `held` does not keep, closed once here.
                    unsafe { CloseHandle(next) };
                    return false;
                }
                // SAFETY: `next` is an open handle to another thread of this
                // process, opened with `THREAD_SUSPEND_RESUME`.
                if unsafe { SuspendThread(next) } != u32::MAX {
                    // A suspension only takes hold once the thread's context
                    // is read.
                    let _settled = control(next);
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
        // SAFETY: `GetCurrentProcessId` takes no arguments and only reads the
        // calling process's id.
        let process = unsafe { GetCurrentProcessId() };
        // SAFETY: `TH32CS_SNAPTHREAD` with process 0 asks for every thread of
        // the system and reads no memory of the caller's.
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
        // SAFETY: `snapshot` is the open snapshot checked against
        // `INVALID_HANDLE_VALUE`, and `entry` is a local whose `dwSize` holds
        // its size, as `Thread32First` requires.
        let mut more = unsafe { Thread32First(snapshot, &raw mut entry) } != 0;
        while more && self.held.len() < self.held.capacity() {
            let id = entry.th32ThreadID;
            if entry.th32OwnerProcessID == process && id != me {
                // SAFETY: `OpenThread` takes any thread id and returns null
                // for one it cannot open; `id` belongs to this process.
                let thread = unsafe { OpenThread(access, 0, id) };
                // SAFETY: a thread of our own process, opened with suspend
                // rights; a null handle fails the call.
                if !thread.is_null() && unsafe { SuspendThread(thread) } != u32::MAX {
                    let _settled = control(thread);
                    self.held.push((thread, id));
                } else if !thread.is_null() {
                    // SAFETY: `thread` is the non-null handle `OpenThread`
                    // returned in this iteration, which `held` does not keep.
                    unsafe { CloseHandle(thread) };
                }
            }
            // SAFETY: `snapshot` is still the open snapshot, and `entry` is
            // the local whose `dwSize` holds its size.
            more = unsafe { Thread32Next(snapshot, &raw mut entry) } != 0;
        }
        // SAFETY: `snapshot` is the open snapshot this function created, closed
        // once here.
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
            // SAFETY: every handle in `held` is open, with
            // `THREAD_SUSPEND_RESUME`, and was suspended once by `suspend` or
            // `suspend_listed`.
            unsafe { ResumeThread(thread) };
            // SAFETY: `drain` hands each open handle of `held` out once, and
            // nothing keeps it after this.
            unsafe { CloseHandle(thread) };
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::*;

    #[test]
    fn only_mapped_code_takes_a_hook() {
        let code = (only_mapped_code_takes_a_hook as fn() as *const ()).addr();
        assert!(hookable(code, READ));
        let data = 7u64;
        assert!(!hookable(core::ptr::from_ref(&data).addr(), READ));
        assert!(!hookable(0, READ));
        assert!(!hookable(0x10, READ));
        assert!(!hookable(usize::MAX - 4, READ));
        assert!(matches!(
            prepare_one(
                Request {
                    target: 0x10,
                    detour: code,
                    inside: false,
                },
                &mut Vec::new()
            ),
            Err(why) if why.contains("not mapped")
        ));
    }

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
            // SAFETY: `GetCurrentThreadId` takes no arguments and only reads
            // the calling thread's id.
            let me = unsafe { GetCurrentThreadId() };
            threads.suspend_listed(me, THREAD_SUSPEND_RESUME)
        }));
    }
}
