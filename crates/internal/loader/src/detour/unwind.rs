//! Unwind data for the relays and trampolines, so stack walks and exceptions
//! pass a frame stopped in them.

use enshwick::abi::Level;
use windows_sys::Win32::System::Diagnostics::Debug::{
    IMAGE_RUNTIME_FUNCTION_ENTRY, RtlInstallFunctionTableCallback,
};

use super::{BLOCK, CODE, ENTRY, FAR_JUMP, Function, reads};
use crate::settings::LOADER_ID;
use crate::unwind_info::{
    CHAINED, CODES, RuntimeFunction, SLOT, prologue_code, prologue_offset, slots,
};

/// The bytes a [`RuntimeFunction`] takes in the entry, before its unwind info.
const TABLE_ENTRY: usize = size_of::<RuntimeFunction>();
/// Where a chained entry holds the next unwind info, after start and end.
const UNWIND_OF: usize = core::mem::offset_of!(RuntimeFunction, unwind);

/// Writes the function-table entry and unwind info of an entry at `at`, its
/// offset in the block it sits in, whose relay and trampoline take `length`
/// bytes, into `out`, the entry's bytes after its code. The hook stole
/// `stolen` bytes from `target`.
///
/// A target with no function entry (a leaf, such as generated code) gets no
/// entry and unwinds as a leaf, as it did before the hook. So does one whose
/// unwind info cannot be read or copied, with a warning: unwind data is an
/// aid, never a reason to leave a hook uninstalled.
pub(super) fn fill(
    at: usize,
    length: usize,
    target: usize,
    stolen: usize,
    function: Option<Function>,
    out: &mut [u8],
) {
    let Some(function) = function else {
        return;
    };
    if function_unwind(target, stolen, function, out).is_none() {
        crate::log!(
            WARN,
            "hook at {target:#x}: its function's unwind data could not be read; a stack walk through its trampoline may be wrong"
        );
        return;
    }
    let rva = |offset: usize| u32::try_from(at + offset).unwrap_or(0);
    let entry = RuntimeFunction {
        begin: rva(0),
        end: rva(length),
        unwind: rva(CODE + TABLE_ENTRY),
    };
    // SAFETY: `RuntimeFunction` is `repr(C)` of three `u32`, so its bytes are
    // `TABLE_ENTRY` initialised bytes, and `out` is the entry's room, at
    // least `TABLE_ENTRY` long since `function_unwind` wrote past it.
    let bytes = unsafe {
        core::slice::from_raw_parts(core::ptr::from_ref(&entry).cast::<u8>(), TABLE_ENTRY)
    };
    out[..TABLE_ENTRY].copy_from_slice(bytes);
}

/// Hands the unwinder the entries of the sealed block at `block`.
pub(super) fn register(block: usize) {
    // SAFETY: `block` is a sealed block that is never freed, and `entry_of`
    // only reads it.
    let added = unsafe {
        RtlInstallFunctionTableCallback(
            block as u64 | 3,
            block as u64,
            u32::try_from(BLOCK).unwrap_or(0),
            Some(entry_of),
            core::ptr::with_exposed_provenance(block),
            core::ptr::null(),
        )
    };
    if !added {
        crate::log::write(
            Level::WARN,
            LOADER_ID,
            &format!("hook trampolines at {block:#x} have no unwind data"),
        );
    }
}

/// The function-table entry of the relay and trampoline at `pc` in the block
/// at `block`, for the unwinder; null where none is written.
///
/// The returned pointer is into the sealed block, owned by the loader and
/// never freed, so the unwinder may read it for the life of the process; a
/// null return means the address has no entry. The unwinder calls this on
/// any thread, during exception dispatch and in the crash hook, so it only
/// reads and never panics or allocates.
///
/// # Safety
///
/// - `block` is the context [`register`] gave the unwinder: the base of a
///   sealed block of [`BLOCK`] bytes that is never freed.
/// - `pc` is an address the unwinder is resolving, which may be anything.
unsafe extern "system" fn entry_of(
    pc: u64,
    block: *const core::ffi::c_void,
) -> *mut IMAGE_RUNTIME_FUNCTION_ENTRY {
    let block = block.addr();
    let Some(offset) = usize::try_from(pc)
        .ok()
        .and_then(|pc| pc.checked_sub(block))
        .filter(|&offset| offset < BLOCK)
    else {
        return core::ptr::null_mut();
    };
    let entry = block + offset - offset % ENTRY + CODE;
    // SAFETY: `entry` lies in the sealed, readable block `register` handed
    // the unwinder with this callback, which is never freed or written again.
    let [begin, end, _]: [u32; 3] = unsafe { enshwick::raw::read_at(entry as *const u8, 0) };
    if (begin as usize..end as usize).contains(&offset) {
        core::ptr::with_exposed_provenance_mut(entry)
    } else {
        core::ptr::null_mut()
    }
}

/// Writes the unwind info for `function`, whose hook stole `stolen` bytes
/// from `target`, into `out` after the room for its function-table entry;
/// `None` when its info cannot be read or does not fit.
///
/// The info holds the codes of the function, its chained entries flattened
/// in, without their epilogue codes or handler: a frame stopped in the relay
/// or the trampoline unwinds as one stopped at the same place in the
/// function. The prologue length is set to 255, past the whole entry, so the
/// unwinder always treats a trampoline address as inside the prologue and
/// never scans for an epilogue (x64 exception handling, "RIP is within the
/// prolog if the distance from the function start to the RIP is less than or
/// equal to the prolog size"; then "the first entry with an offset less than
/// or equal to the offset of the RIP" and those after it apply). Each code's
/// offset is where its instruction has run in the trampoline, so a frame
/// before the moved bytes unwinds as the function's entry and one after them
/// as past that much of the prologue.
fn function_unwind(target: usize, stolen: usize, function: Function, out: &mut [u8]) -> Option<()> {
    /// The most chained entries followed, as the crash walk does.
    const CHAIN: usize = 32;
    let (header, codes) = out.get_mut(TABLE_ENTRY..)?.split_at_mut(CODES);
    let (mut written, mut frame, mut rva) = (0usize, 0u8, function.unwind);
    for depth in 0..CHAIN {
        let at = function
            .image
            .checked_add(rva as usize)
            .filter(|&at| reads(at, CODES))?;
        // SAFETY: `reads` checked the four bytes at `at`, in the image the
        // function table names, which stays loaded.
        let [flags, _, count, frame_here]: [u8; 4] =
            unsafe { enshwick::raw::read_at(at as *const u8, 0) };
        if !matches!(flags & 7, 1 | 2) {
            return None;
        }
        let chained = (flags >> 3) & CHAINED != 0;
        let count = usize::from(count);
        let next = CODES + count.next_multiple_of(2) * SLOT;
        if !reads(at, next + if chained { TABLE_ENTRY } else { 0 }) {
            return None;
        }
        if frame == 0 {
            frame = frame_here;
        }
        let slot = |i: usize| -> [u8; 2] {
            // SAFETY: `reads` checked every code slot below `count`, the
            // only ones read, in the image that stays loaded.
            unsafe { enshwick::raw::read_at(at as *const u8, CODES + i * SLOT) }
        };
        let mut i = 0;
        while i < count {
            let [offset, op] = slot(i);
            let taken = slots(op & 0xf, op >> 4)?;
            if i + taken > count {
                return None;
            }
            if prologue_code(op & 0xf) {
                let room = codes.get_mut(written..written + taken * SLOT)?;
                for (k, pair) in room.as_chunks_mut::<2>().0.iter_mut().enumerate() {
                    *pair = slot(i + k);
                }
                // A chained entry's codes describe the primary prologue,
                // already run on entry to the trampoline: offset zero.
                room[0] = if depth == 0 {
                    prologue_offset(
                        FAR_JUMP,
                        target,
                        stolen,
                        function.begin + usize::from(offset),
                    )
                } else {
                    0
                };
                written += taken * SLOT;
            }
            i += taken;
        }
        if !chained {
            let count = u8::try_from(written / SLOT).ok()?;
            header.copy_from_slice(&[1, u8::MAX, count, frame]);
            return Some(());
        }
        // SAFETY: `reads` checked the chained entry after the codes: a
        // `RuntimeFunction` whose `unwind` field is the next unwind info, an
        // offset from the image.
        rva = unsafe { enshwick::raw::read_at(at as *const u8, next + UNWIND_OF) };
    }
    None
}
