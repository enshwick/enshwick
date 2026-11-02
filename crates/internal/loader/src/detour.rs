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
//! 1269579). On the client (build 426) no module of the process, drivers
//! and overlays included, called a fiber function from the main menu
//! through a world load; a fiber another module makes stays unseen.
//!
//! Code may also jump into the replaced bytes after the first, where it
//! would land inside the jump. A target is refused when a direct branch or
//! call of its own function, as the image's function table bounds it, lands
//! there, decoding the function from its start. Not seen: indirect jumps
//! (switch tables, computed addresses), a branch hidden by data inside the
//! function's bounds that the decoder reads as instructions,
//! branches from other functions or from parts of the same function the
//! compiler split into entries of their own, and code without a function
//! entry, such as generated code. A wrong site then fails only when that
//! path runs.
//!
//! Each relay and trampoline carries unwind data copied from its target's
//! function, so stack walks and exceptions pass a frame stopped there, such
//! as one that called out of the moved instructions. The copy sets a
//! prologue length of 255, past the whole entry, so the unwinder always
//! treats a trampoline address as inside the prologue and never scans for an
//! epilogue (x64 exception handling,
//! `learn.microsoft.com/cpp/build/exception-handling-x64`); each code's
//! offset is where its instruction has run in the trampoline, so a frame on
//! the relay unwinds as the function's entry and one past the moved bytes as
//! past that much of the prologue. The function's own exception handler is
//! not copied: an exception raised below a moved call passes that function
//! by, as if it had no handler. A function whose unwind data cannot be read
//! is still hooked, without unwind data, as a leaf.

// Long file: the inline-hook engine is one tightly-coupled unsafe algorithm
// (prepare, batched write with threads suspended, thread relocation) over
// shared private invariants; the thread-suspension and relocation machinery
// is to split out with the frame-walk settle work that follows.

use std::sync::Mutex;

use enshwick::abi::Level;
use iced_x86::{
    BlockEncoder, BlockEncoderOptions, Code, Decoder, DecoderOptions, FlowControl, Instruction,
    InstructionBlock,
};
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Diagnostics::Debug::{
    CONTEXT, CONTEXT_CONTROL_AMD64, GetThreadContext, RtlLookupFunctionEntry, SetThreadContext,
};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
};
use windows_sys::Win32::System::Memory::PAGE_EXECUTE_READWRITE;
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentProcessId, GetCurrentThreadId, GetThreadId, OpenThread,
    ResumeThread, SuspendThread, THREAD_GET_CONTEXT, THREAD_QUERY_LIMITED_INFORMATION,
    THREAD_SET_CONTEXT, THREAD_SUSPEND_RESUME,
};

use crate::locks::Relock;
use crate::pages;
use crate::settings::LOADER_ID;

/// The jump written over a target: `jmp rel32`.
const PATCH: usize = 5;
/// `jmp [rip + 0]` followed by the 64-bit destination.
const FAR_JUMP: usize = 14;
/// Bytes read from a target to find the instructions to move.
const READ: usize = 32;
/// A relay and a trampoline: moved instructions grow at most to this.
const CODE: usize = 128;
/// A relay and a trampoline, then their function-table entry and unwind
/// info.
const ENTRY: usize = 256;
/// Windows reserves memory in 64 KiB steps.
const GRANULARITY: usize = 0x1_0000; // bound, not a game offset: a fact of Windows
const BLOCK: usize = GRANULARITY;
/// How far a `rel32` reaches, kept clear of the edge for the block's length.
const REACH: usize = 0x7fff_0000 - BLOCK; // bound, not a game offset: a fact of x86-64
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

/// Why a target is left alone instead of hooked.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(crate) enum PrepareError {
    /// The relay pages could not be made executable.
    #[error("relay pages not executable")]
    RelayNotExecutable,
    /// The target address is not in mapped code.
    #[error("its address is not mapped code")]
    NotMappedCode,
    /// The target's first instructions do not decode.
    #[error("its first instructions do not decode")]
    Undecodable,
    /// The function ends before the jump would.
    #[error("the function is shorter than the jump")]
    TooShort,
    /// A branch leaves from inside the bytes the jump replaces.
    #[error("control leaves within the bytes of the jump")]
    ControlLeaves,
    /// The first instructions cannot be re-encoded at the trampoline.
    #[error("its first instructions cannot move: {0}")]
    CannotMove(String),
    /// The moved instructions do not fit the trampoline.
    #[error("its first instructions grow too long to move")]
    GrowsTooLong,
    /// No relay lies within a 32-bit jump of the target.
    #[error("no relay within reach")]
    NoRelayInReach,
    /// No free memory lies within a 32-bit jump of the target.
    #[error("no free memory within reach of the target")]
    NoMemoryInReach,
    /// The jump would overwrite bytes another hook's jump takes.
    #[error("its bytes overlap another hook's jump")]
    OverlapsAnotherHook,
    /// A branch of the target's function, at this address, lands inside the
    /// bytes the jump replaces, where it would land in the jump itself.
    #[error("the branch at {0:#x} lands inside the bytes of the jump")]
    JumpedInto(usize),
}

/// Prepares every request: relays and trampolines written and sealed, the
/// targets untouched. Each result carries the trampoline that calls the
/// original function, or why the target is left alone.
pub(crate) fn prepare(requests: &[Request]) -> Vec<Result<Pending, PrepareError>> {
    let mut fresh: Vec<usize> = Vec::new();
    let prepared: Vec<Result<Pending, PrepareError>> = requests
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
                .map(|_| Err(PrepareError::RelayNotExecutable))
                .collect();
        }
    }
    for &block in &fresh {
        unwind::register(block);
    }
    prepared
}

/// Bytes to write over game code in the same batch as the hooks.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Bytes<'a> {
    /// The caller's own id for the patch, which [`Commit::Busy`] reports.
    pub(crate) id: usize,
    /// Where they go.
    pub(crate) address: usize,
    /// What goes there.
    pub(crate) replacement: &'a [u8],
}

impl Pending {
    /// The bytes of the target the hook replaces or moves: start and length.
    pub(crate) fn span(&self) -> (usize, usize) {
        (self.target, self.stolen)
    }
}

/// What [`commit`] did with a batch.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Commit {
    /// Every jump and code patch is written.
    Written,
    /// Nothing is written: a target could not be made writable, or a
    /// thread's context could not be read to check and move it.
    Failed,
    /// Nothing is written: a thread stood or returned inside these code
    /// patches, by their [`Bytes::id`], through every wait. A code patch has
    /// no trampoline to move it to, so the batch goes again without them.
    Busy(Vec<usize>),
}

/// Writes the jumps over the targets of `pending` and the `code` patches,
/// all or none. On [`Commit::Busy`], `disarm` runs while the other threads
/// still stand still, so it must neither allocate, lock nor log.
pub(crate) fn commit(pending: &[&Pending], code: &[Bytes<'_>], disarm: fn()) -> Commit {
    write_all(pending, code, disarm)
}

/// Whether `address` is mapped code followed by `len` bytes that read: a
/// wrong address from a mod or a table must refuse its hook, not fault.
fn hookable(address: usize, len: usize) -> bool {
    let Some(end) = address.checked_add(len) else {
        return false;
    };
    pages::query(address)
        .filter(pages::Region::runs)
        .is_some_and(|first| reads_to(first.end, end))
}

fn prepare_one(request: Request, fresh: &mut Vec<usize>) -> Result<Pending, PrepareError> {
    if !hookable(request.target, READ) {
        return Err(PrepareError::NotMappedCode);
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
            return Err(PrepareError::Undecodable);
        }
        // Bytes after a return or a jump away belong to other code, which
        // the jump must not overwrite.
        match instruction.flow_control() {
            FlowControl::Return | FlowControl::Interrupt | FlowControl::Exception
                if !request.inside && stolen + instruction.len() < PATCH =>
            {
                return Err(PrepareError::TooShort);
            }
            FlowControl::Return
            | FlowControl::Interrupt
            | FlowControl::Exception
            | FlowControl::UnconditionalBranch
            | FlowControl::IndirectBranch
                if stolen + instruction.len() < PATCH =>
            {
                return Err(PrepareError::ControlLeaves);
            }
            _ => {}
        }
        stolen += instruction.len();
        instructions.push(instruction);
    }
    let function = function_around(request.target);
    if let Some(branch) = jumped_into(request.target, stolen, function) {
        return Err(PrepareError::JumpedInto(branch));
    }
    let (base, entry) = near(request.target, fresh)?;
    let (relay, trampoline) = (entry, entry + FAR_JUMP);
    let back = request.target + stolen;
    let moved_count = instructions.len();
    // The jump back goes through the encoder, as the block's last
    // instruction, so the encoder lays any far-call target data after it,
    // where execution never falls and no return address lands.
    instructions.push(
        Instruction::with_branch(Code::Jmp_rel32_64, back as u64)
            .map_err(|e| PrepareError::CannotMove(e.to_string()))?,
    );
    let block = InstructionBlock::new(&instructions, trampoline as u64);
    let encoded = BlockEncoder::encode(
        64,
        block,
        BlockEncoderOptions::RETURN_NEW_INSTRUCTION_OFFSETS,
    )
    .map_err(|e| PrepareError::CannotMove(e.to_string()))?;
    let length = FAR_JUMP + encoded.code_buffer.len();
    if length > CODE {
        return Err(PrepareError::GrowsTooLong);
    }
    // The jump back is the first instruction after the moved bytes and is
    // never rewritten, so its offset bounds the moved region.
    let moved_end = usize::try_from(encoded.new_instruction_offsets[moved_count])
        .ok()
        .filter(|&end| end <= encoded.code_buffer.len())
        .ok_or_else(|| PrepareError::CannotMove("the jump back did not encode".into()))?;
    // The encoder reports `u32::MAX` for an instruction it rewrote into a far
    // form, so the moved instructions' trampoline offsets come from decoding
    // the moved region; a rewrite that split one into several (a branch out of
    // reach) leaves a different count, and the target is refused.
    let moved = moved_offsets(
        &encoded.code_buffer[..moved_end],
        &instructions[..moved_count],
    )?;
    let mut bytes = [0; ENTRY];
    let (code, unwind) = bytes.split_at_mut(CODE);
    code[..FAR_JUMP].copy_from_slice(&far_jump(request.detour));
    code[FAR_JUMP..length].copy_from_slice(&encoded.code_buffer);
    unwind::fill(
        entry - base,
        length,
        request.target,
        stolen,
        function,
        unwind,
    );
    // SAFETY: `entry` has `ENTRY` writable bytes in a block not yet sealed.
    unsafe { (entry as *mut u8).copy_from_nonoverlapping(bytes.as_ptr(), ENTRY) };
    let distance = i32::try_from(relay.wrapping_sub(request.target + PATCH).cast_signed())
        .map_err(|_| PrepareError::NoRelayInReach)?;
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

/// Each moved instruction's start offset in the target and in the trampoline,
/// by decoding the `region` the moved instructions occupy in the trampoline.
/// Refuses when the encoder split one into several (a branch rewritten out of
/// reach), which leaves more instructions than were moved.
fn moved_offsets(
    region: &[u8],
    moved: &[Instruction],
) -> Result<Vec<(usize, usize)>, PrepareError> {
    let mut decoder = Decoder::with_ip(64, region, 0, DecoderOptions::NONE);
    let starts: Vec<usize> = decoder
        .iter()
        .map(|instruction| usize::try_from(instruction.ip()).unwrap_or(usize::MAX))
        .collect();
    if starts.len() != moved.len() {
        return Err(PrepareError::CannotMove(
            "a moved branch was rewritten into several instructions".into(),
        ));
    }
    let mut old = 0;
    Ok(moved
        .iter()
        .zip(starts)
        .map(|(instruction, new)| {
            let pair = (old, new);
            old += instruction.len();
            pair
        })
        .collect())
}

/// The first direct branch or call of `function` that lands in
/// `target + 1..target + stolen`, where it would land in the jump itself. A
/// branch among the moved instructions moves with them, its target in the
/// trampoline. Code without a function entry, a leaf such as generated code,
/// is not checked.
fn jumped_into(target: usize, stolen: usize, function: Option<Function>) -> Option<usize> {
    let Function { begin, end, .. } = function?;
    let len = end.checked_sub(begin)?;
    if !hookable(begin, len) {
        return None;
    }
    // SAFETY: `hookable` checked that the function's bytes lie in committed,
    // readable pages of an image that stays loaded: hooks arm while the
    // loader holds the game's and the mods' images, which it never unloads.
    // The slice lives for this function only, while no patch writes them.
    let code = unsafe { core::slice::from_raw_parts(begin as *const u8, len) };
    let moved = target as u64..(target + stolen) as u64;
    let inside = target as u64 + 1..(target + stolen) as u64;
    let mut decoder = Decoder::with_ip(64, code, begin as u64, DecoderOptions::NONE);
    decoder
        .iter()
        .find(|i| !moved.contains(&i.ip()) && inside.contains(&i.near_branch_target()))
        .and_then(|instruction| usize::try_from(instruction.ip()).ok())
}

/// The function whose entry in its image's function table holds `address`;
/// `None` without one.
fn function_around(address: usize) -> Option<Function> {
    let mut image = 0;
    // SAFETY: `RtlLookupFunctionEntry` takes any address, `image` is a
    // writable `u64` and the history table may be null.
    let entry =
        unsafe { RtlLookupFunctionEntry(address as u64, &raw mut image, core::ptr::null_mut()) };
    if entry.is_null() {
        return None;
    }
    let image = usize::try_from(image).ok()?;
    // SAFETY: a non-null entry is a readable `IMAGE_RUNTIME_FUNCTION_ENTRY`
    // of a loaded image or a registered table, kept while the image is: the
    // function's start, end and unwind info, offsets from `image`. Hooks arm
    // while the game's and the mods' images stay loaded.
    let [begin, end, unwind]: [u32; 3] = unsafe { enshwick::raw::read_at(entry.cast(), 0) };
    Some(Function {
        image,
        begin: image + begin as usize,
        end: image + end as usize,
        unwind,
    })
}

/// A function as its image's function table records it.
#[derive(Clone, Copy, Debug)]
pub(super) struct Function {
    pub(super) image: usize,
    pub(super) begin: usize,
    pub(super) end: usize,
    /// Its unwind info, an offset from `image`.
    pub(super) unwind: u32,
}

/// Whether the `len` bytes from `address` all lie in readable pages.
fn reads(address: usize, len: usize) -> bool {
    address
        .checked_add(len)
        .is_some_and(|end| reads_to(address, end))
}

fn reads_to(mut reached: usize, end: usize) -> bool {
    while reached < end {
        match pages::query(reached) {
            Some(next) if next.reads() && next.end > reached => reached = next.end,
            _ => return false,
        }
    }
    true
}

/// `jmp [rip + 0]` to `destination`.
fn far_jump(destination: usize) -> [u8; FAR_JUMP] {
    let mut bytes = [0xff, 0x25, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    bytes[6..].copy_from_slice(&(destination as u64).to_le_bytes());
    bytes
}

/// `ENTRY` bytes within `rel32` reach of `target`, and the base of the block
/// they sit in, both from a block allocated in this batch.
fn near(target: usize, fresh: &mut Vec<usize>) -> Result<(usize, usize), PrepareError> {
    let mut blocks = BLOCKS.relock();
    let reaches = |base: usize| base.abs_diff(target) < REACH;
    if let Some((base, used)) = blocks
        .iter_mut()
        .find(|(base, used)| fresh.contains(base) && reaches(*base) && *used + ENTRY <= BLOCK)
    {
        let entry = *base + *used;
        *used += ENTRY;
        return Ok((*base, entry));
    }
    let base = allocate_near(target).ok_or(PrepareError::NoMemoryInReach)?;
    fresh.push(base);
    blocks.push((base, ENTRY));
    Ok((base, base))
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
            pages::allocate(Some(address), BLOCK).map(|block| block.as_ptr().addr())
        })
}

/// The warning for a thread whose stack the settle could not unwind.
const SCANNED: &str = "a thread's stack did not unwind during the settle; its return addresses into code patches were found by a conservative stack scan";

/// Writes every hook jump and code patch with the other threads suspended,
/// or none of them.
///
/// A suspended thread may stand inside the bytes a hook replaces, or have
/// called out of them, with a return address into them on its stack; both
/// move to the same instruction in the trampoline. A code patch has no
/// trampoline, so the writes wait, a few times, until no thread stands or
/// returns inside one; a patch a thread never leaves is never written over
/// it, since the thread would resume in the middle of other instructions.
fn write_all(prepared: &[&Pending], code: &[Bytes<'_>], disarm: fn()) -> Commit {
    let writes: Vec<(usize, &[u8])> = prepared
        .iter()
        .map(|&p| (p.target, &p.patch[..]))
        .chain(code.iter().map(|c| (c.address, c.replacement)))
        .collect();
    if writes.is_empty() {
        return Commit::Written;
    }
    let moved = moved_instructions(prepared);
    let patched: Vec<(usize, usize)> = code
        .iter()
        .map(|c| (c.address, c.address + c.replacement.len()))
        .collect();
    let returns = patch_returns(code);
    // Nothing below allocates while the other threads stand still: one of
    // them may hold the heap's lock.
    let mut protections = Vec::with_capacity(writes.len());
    let mut stuck = vec![false; code.len()];
    let mut scanned = false;
    let mut threads = Threads::new();
    let mut tries = 0;
    let checked = !moved.is_empty() || !patched.is_empty();
    let blind = loop {
        if !threads.suspend() {
            threads.resume();
            return Commit::Failed;
        }
        stuck.fill(false);
        let last = tries == SETTLE_TRIES;
        let mut blind = None;
        if checked {
            for thread in threads.iter() {
                // SAFETY: `suspend` just suspended every other thread of the
                // process, `iter` yields each of them, and they stay
                // suspended until `resume`.
                if !unsafe { mark(thread, &patched, &returns, &mut stuck, &mut scanned) } {
                    // A thread whose context Windows does not hand over can be
                    // neither checked nor moved: the writes wait for it, then
                    // give up.
                    // SAFETY: `GetThreadId` only reads the id of a thread
                    // handle `suspend` opened with
                    // `THREAD_QUERY_LIMITED_INFORMATION`.
                    blind = Some(unsafe { GetThreadId(thread) });
                    break;
                }
                // A pass that waits again needs only one stuck thread; the
                // last pass marks every patch each thread stays in.
                if !last && stuck.contains(&true) {
                    break;
                }
            }
        }
        if !(stuck.contains(&true) || blind.is_some()) || last {
            break blind;
        }
        threads.resume();
        tries += 1;
        std::thread::sleep(SETTLE_WAIT);
    };
    if let Some(thread) = blind {
        threads.resume();
        crate::log::write(
            Level::WARN,
            LOADER_ID,
            &format!(
                "hooks not written: the context of thread {thread} could not be read to move it out of the code"
            ),
        );
        return Commit::Failed;
    }
    if stuck.contains(&true) {
        disarm();
        threads.resume();
        if scanned {
            crate::log::write(Level::WARN, LOADER_ID, SCANNED);
        }
        return Commit::Busy(
            code.iter()
                .zip(&stuck)
                .filter_map(|(patch, &s)| s.then_some(patch.id))
                .collect(),
        );
    }
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
    if scanned {
        crate::log::write(Level::WARN, LOADER_ID, SCANNED);
    }
    if writable {
        Commit::Written
    } else {
        Commit::Failed
    }
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
/// as it is now, each with its patch's index, sorted: where a thread that
/// called out of a patch returns.
fn patch_returns(code: &[Bytes<'_>]) -> Vec<(usize, usize)> {
    let mut returns: Vec<(usize, usize)> = code
        .iter()
        .enumerate()
        .flat_map(|(i, c)| {
            (c.address + 1..c.address + c.replacement.len())
                .filter(|&at| follows_call(c.address, at))
                .map(move |at| (at, i))
        })
        .collect();
    returns.sort_unstable();
    returns.dedup_by_key(|&mut (at, _)| at);
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

/// A thread's `CONTEXT` on the 16-byte boundary x64 Windows requires:
/// `GetThreadContext` and `SetThreadContext` refuse a misaligned one, which
/// the windows-sys type, aligned to 8, does not prevent; Wine does not check.
#[repr(C, align(16))]
#[derive(Clone, Copy)]
pub(crate) struct ThreadContext(pub(crate) CONTEXT);

impl ThreadContext {
    /// An empty context that asks for the control registers.
    pub(crate) fn new() -> Self {
        Self(CONTEXT {
            ContextFlags: CONTEXT_CONTROL_AMD64,
            ..CONTEXT::default()
        })
    }
}

/// A suspended thread's stack pointer and instruction pointer, with the
/// context read for writing it back; `None` when Windows refuses to read
/// it.
fn control(thread: HANDLE) -> Option<ThreadContext> {
    let mut context = ThreadContext::new();
    // SAFETY: `GetThreadContext` writes at most one `CONTEXT` into
    // `context.0`, 16-byte aligned by its type, with `ContextFlags` set; a
    // handle without context rights fails the call.
    (unsafe { GetThreadContext(thread, &raw mut context.0) } != 0).then_some(context)
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

    /// Hands each word to `visit`.
    ///
    /// # Safety
    ///
    /// - Every thread of the process but the caller stays suspended for the
    ///   call, as `Threads::suspend` leaves them, so the stack's region stays
    ///   committed and no other thread writes it.
    unsafe fn each(self, mut visit: impl FnMut(usize)) {
        for word in 0..self.words {
            // SAFETY: `word` is below `words`, so its eight bytes lie in the
            // committed read-write region `at` found, which stays committed
            // and unwritten while every other thread stays suspended.
            visit(unsafe { crate::memory::read_stack_word(self.address(word)) });
        }
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
            let old = unsafe { crate::memory::read_stack_word(at) };
            if let Some(value) = new(old) {
                // SAFETY: the same eight bytes, writable and touched by no
                // other thread while every other thread stays suspended; the
                // caller vouches for `value`.
                unsafe { crate::memory::write_stack_word(at, value) };
            }
        }
    }
}

/// Marks in `stuck` each patched range a suspended thread stands inside or
/// returns into, found by unwinding its real frames over the game's and the
/// loader's unwind tables. A thread whose stack does not unwind to the top
/// falls back to a conservative scan of every stack word against the return
/// addresses `returns` pairs with each patch's index, and sets `scanned`,
/// for a warning once the threads resume. False when the context does not
/// read.
///
/// # Safety
///
/// - Every thread of the process but the caller, `thread` among them, stays
///   suspended for the call, as `Threads::suspend` leaves them, so its stack
///   does not change while this walks or scans it.
unsafe fn mark(
    thread: HANDLE,
    patched: &[(usize, usize)],
    returns: &[(usize, usize)],
    stuck: &mut [bool],
    scanned: &mut bool,
) -> bool {
    let Some(ThreadContext(context)) = control(thread) else {
        return false;
    };
    let mut hit = |address: usize| {
        for (flag, &(start, end)) in stuck.iter_mut().zip(patched) {
            *flag |= start < address && address < end;
        }
    };
    hit(usize::try_from(context.Rip).unwrap_or(0));
    let mut frames = [0usize; crate::crash::FRAMES];
    // SAFETY: the caller keeps `thread` suspended, so its stack stays put for
    // the walk, which reads only committed stack and the loaded images.
    let (found, complete) = unsafe { crate::crash::unwind::unwind(&context, &mut frames) };
    if complete {
        frames[..found].iter().for_each(|&address| hit(address));
        return true;
    }
    // The stack did not unwind; every word that looks like a return address
    // into a patch counts, lest one be missed.
    *scanned = true;
    let (Some(&(low, _)), Some(&(high, _))) = (returns.first(), returns.last()) else {
        return true;
    };
    if let Some(stack) = Stack::at(context.Rsp) {
        // SAFETY: the caller keeps every other thread, the stack's owner
        // among them, suspended for the call.
        unsafe {
            stack.each(|word| {
                if (low..=high).contains(&word)
                    && let Ok(i) = returns.binary_search_by_key(&word, |&(at, _)| at)
                    && let Some(flag) = stuck.get_mut(returns[i].1)
                {
                    *flag = true;
                }
            });
        }
    }
    true
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
    let Some(mut held) = control(thread) else {
        return;
    };
    if let Some(new) = find(usize::try_from(held.0.Rip).unwrap_or(0)) {
        held.0.Rip = new as u64;
        // SAFETY: the caller keeps `thread` suspended, `held.0` is the one
        // `control` read from it with only `Rip` changed, and the moved
        // address holds the same instruction in the trampoline.
        unsafe { SetThreadContext(thread, &raw const held.0) };
    }
    if let Some(stack) = Stack::at(held.0.Rsp) {
        // SAFETY: the caller keeps every other thread, the stack's owner
        // among them, suspended for the call and vouches for the moved
        // addresses. A word equal to a moved address is a return address, or
        // a stale copy of one: either way the trampoline holds the same
        // instruction.
        unsafe { stack.rewrite(find) };
    }
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
mod tests;
mod unwind;
